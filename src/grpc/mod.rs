use std::collections::HashMap;
use std::sync::Arc;

use sqlx::PgPool;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tonic::{Code, Request, Response, Status};
use tracing::{error, info};

use crate::healthcheck::HealthcheckService;
use crate::{db, protobuf};
use crate::{Empty, ListQuery, QueryFilter};
use crate::{Healthcheck, HealthcheckList, HealthcheckResultList, ResultQuery};

mod util;

#[derive(Debug)]
pub struct Server {
    db_client: Arc<PgPool>,
    // Tokio Mutex is used since we hold the guard over await statements
    handlers: Mutex<HashMap<i32, JoinHandle<()>>>,
}

impl Server {
    pub fn new(db_client: Arc<PgPool>, handlers: Mutex<HashMap<i32, JoinHandle<()>>>) -> Server {
        Server {
            db_client,
            handlers,
        }
    }
}

// Tower takes care of gRPC access logging, so only special events are reported
#[tonic::async_trait]
impl protobuf::photon_gun_server::PhotonGun for Server {
    async fn get_healthcheck(
        &self,
        query: Request<QueryFilter>,
    ) -> Result<Response<Healthcheck>, Status> {
        let id = util::id_from_query_filter(query.into_inner())?;

        let res = match db::get_healthcheck(&self.db_client, id).await {
            Ok(val) => val,
            Err(err) => {
                error!(%err);
                return Err(Status::new(Code::Internal, err.to_string()));
            }
        };

        Ok(Response::new(res))
    }

    async fn list_healthchecks(
        &self,
        query: Request<ListQuery>,
    ) -> Result<Response<HealthcheckList>, Status> {
        let query = query.into_inner();
        let enabled = query.enabled.unwrap_or(true);
        let limit = query.limit.unwrap_or(10);

        let res = match db::list_healthchecks(&self.db_client, enabled, limit).await {
            Ok(val) => val,
            Err(err) => {
                error!(%err);
                return Err(Status::new(Code::Internal, err.to_string()));
            }
        };

        Ok(Response::new(res))
    }

    async fn list_healthcheck_results(
        &self,
        query: Request<ResultQuery>,
    ) -> Result<Response<HealthcheckResultList>, Status> {
        let query = query.into_inner();
        let limit = query.limit.unwrap_or(10);

        let res = match db::list_healthcheck_results(&self.db_client, query.id, limit).await {
            Ok(val) => val,
            Err(err) => {
                error!(%err);
                return Err(Status::new(Code::Internal, err.to_string()));
            }
        };

        Ok(Response::new(res))
    }

    async fn create_healthcheck(
        &self,
        check: Request<Healthcheck>,
    ) -> Result<Response<Healthcheck>, Status> {
        // Mutable Justification
        //   ID can only be set after the check has been inserted into the DB
        let mut check = check.into_inner();
        let id = match db::insert_healthcheck(&self.db_client, &check).await {
            Ok(val) => val,
            Err(err) => return Err(Status::new(Code::Internal, err.to_string())),
        };

        info!(healtheck.id = id, msg = "Creating healthcheck...");

        check.id = id;
        check.enabled = true;

        // Clone so we can return the original to the client
        let service = HealthcheckService::new(check.clone(), self.db_client.clone());

        {
            let mut guard = self.handlers.lock().await;
            let handle = service.spawn().await;
            guard.insert(id, handle);
        }

        info!(healthcheck.id = id, msg = "Created.");
        Ok(Response::new(check))
    }

    async fn delete_healthcheck(
        &self,
        query: Request<QueryFilter>,
    ) -> Result<Response<Healthcheck>, Status> {
        let id = util::id_from_query_filter(query.into_inner())?;
        info!(healthcheck.id = id, msg = "Deleting healthcheck...");

        {
            let mut guard = self.handlers.lock().await;
            match guard.remove(&id) {
                Some(handle) => {
                    handle.abort();
                    info!(healthcheck.id = id, msg = "Check stopped.");
                }
                None => info!(healthcheck.id = id, msg = "Check not currently running."),
            };
        }

        let check = match db::delete_healthcheck(&self.db_client, id).await {
            Ok(val) => val,
            Err(err) => return Err(Status::new(Code::Internal, err.to_string())),
        };

        info!(healthcheck.id = id, msg = "Deleted.");
        Ok(Response::new(check))
    }

    async fn enable_healthcheck(
        &self,
        query: Request<QueryFilter>,
    ) -> Result<Response<Healthcheck>, Status> {
        let id = util::id_from_query_filter(query.into_inner())?;

        let check = match db::get_healthcheck(&self.db_client, id).await {
            Ok(val) => val,
            Err(err) => return Err(Status::new(Code::Internal, err.to_string())),
        };

        {
            let mut guard = self.handlers.lock().await;

            // Enable the check in Postgres
            // We do this before actually starting the check since it won't block the check from trying to
            // start while the check start *would* block the database being updated
            if let Err(err) = db::enable_healthcheck(&self.db_client, id).await {
                return Err(Status::new(Code::Internal, err.to_string()));
            }

            // Ensure check isn't already running
            if guard.contains_key(&id) {
                let err = format!("check with id {} is already running", id);
                error!(%err);
                return Err(Status::new(Code::InvalidArgument, err));
            }

            // Start the healthcheck
            let service = HealthcheckService::new(check.clone(), self.db_client.clone());
            let handle = service.spawn().await;

            guard.insert(id, handle);
        }

        Ok(Response::new(check))
    }

    async fn disable_healthcheck(
        &self,
        query: Request<QueryFilter>,
    ) -> Result<Response<Empty>, Status> {
        let id = util::id_from_query_filter(query.into_inner())?;

        {
            let mut guard = self.handlers.lock().await;

            // Disable the check in Postgres
            // Same rationale as the comment block in enable_healthcheck
            if let Err(err) = db::disable_healthcheck(&self.db_client, id).await {
                return Err(Status::new(Code::Internal, err.to_string()));
            }

            // Remove the check from the Hashmap; also checks if check is currently disabled
            let handle = match guard.remove(&id) {
                Some(val) => val,
                None => {
                    let err = format!("check with id {} is not running", id);
                    error!(%err);
                    return Err(Status::new(Code::InvalidArgument, err));
                }
            };

            // Stop the currently running task
            handle.abort();
        }

        Ok(Response::new(Empty {}))
    }
}

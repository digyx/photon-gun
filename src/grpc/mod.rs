use std::sync::Arc;

use sqlx::PgPool;
use tonic::{Code, Request, Response, Status};
use tracing::error;

use crate::{db, protobuf};
use crate::{Healthcheck, PingRequest, PingResponse, Return};

#[derive(Debug)]
pub struct Server {
    db_client: Arc<PgPool>,
}

impl Server {
    pub fn new(db_client: Arc<PgPool>) -> Server {
        Server { db_client }
    }
}

// Tower takes care of gRPC access logging, so only special events are logged
#[tonic::async_trait]
impl protobuf::photon_gun_server::PhotonGun for Server {
    async fn create_healthcheck(
        &self,
        req: Request<Healthcheck>,
    ) -> Result<Response<Return>, Status> {
        let check = req.into_inner();
        let _id = match db::insert_healthcheck(&self.db_client, &check).await {
            Ok(val) => val,
            Err(err) => {
                error!("error: could not access database\n{}", err);
                return Err(Status::new(Code::Internal, err.to_string()));
            }
        };

        Ok(Response::new(Return {
            msg: "Ok".to_string(),
        }))
    }

    async fn ping(&self, _req: Request<PingRequest>) -> Result<Response<PingResponse>, Status> {
        let res = PingResponse {};
        Ok(Response::new(res))
    }
}

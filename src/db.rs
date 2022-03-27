use std::time::{UNIX_EPOCH, SystemTime};

use tokio_postgres::{Client,NoTls};
use tracing::{info,error, debug};

#[derive(Debug)]
pub struct DB{
    client: Client,
    service_name: String,
}

impl DB {
    pub async fn new(service_name: String) -> Result<DB, String> {
        // TODO: Grab postgres credentials from config file
        // TODO: Add TLS option for postgres
        let (client, conn) = match tokio_postgres::connect(
            "postgres://postgres:password@localhost:5432/photon-gun",
            NoTls,
        ).await {
            Ok(client_conn) => {
                info!(target: "database", msg = "connected");
                client_conn
            },
            Err(err) => {
                error!(target: "database", err = err.to_string().as_str());
                return Err(err.to_string())
            },
        };

        // Toss the Postgres connection in the Tokio runtime
        tokio::spawn(async move {
            if let Err(err) = conn.await {
                error!(target: "database", err = format!("{err}").as_str());
            }
        });

        // Each check gets its own table
        // This query creates the table if it doesn't already exist
        let sql_query = format!("
            CREATE TABLE IF NOT EXISTS {service_name} (
                id SERIAL PRIMARY KEY,
                pass BOOLEAN NOT NULL,
                time BIGINT UNIQUE NOT NULL
            )"
        );

        match client.execute(sql_query.as_str(), &[]).await {
            Ok(_) => info!(target: "database", msg = "table created"),
            Err(err) => {
                error!(target: "database", err = format!("{err}").as_str());
                return Err(err.to_string())
            },
        };

        Ok(DB{
            client,
            service_name,
        })
    }

    pub async fn record_healthcheck(&self, pass: bool) -> Result<(), String> {
        let now = SystemTime::now().
            duration_since(UNIX_EPOCH).
            unwrap().
            as_secs() as i64;

        let sql_query = format!("INSERT INTO {} (time, pass) VALUES ($1, $2)", self.service_name);
        debug!(sql_query = sql_query.as_str());

        match self.client.execute(sql_query.as_str(), &[&now, &pass]).await {
             Ok(rows_written) => {
                 debug!(target: "database", rows_written = rows_written);
                 Ok(())
             },
             Err(err) => {
                 error!(target: "database", err = format!("{err}").as_str());
                 Err(err.to_string())
             }
         }
    }
}


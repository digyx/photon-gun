use std::time::{UNIX_EPOCH, SystemTime};

use tokio_postgres::{Client,NoTls};
use tracing::{error,debug};

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
                debug!(msg = "connected");
                client_conn
            },
            Err(err) => {
                error!(error = %err);
                return Err(err.to_string())
            },
        };

        // Toss the Postgres connection in the Tokio runtime
        tokio::spawn(async move {
            if let Err(err) = conn.await {
                error!(error = %err);
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
            Ok(_) => debug!(msg = "table created"),
            Err(err) => {
                error!(error = %err);
                return Err(err.to_string())
            },
        };

        Ok(DB{
            client,
            service_name,
        })
    }

    pub async fn record_basic_check(&self, pass: bool) -> Result<(), tokio_postgres::Error> {
        let now = SystemTime::now().
            duration_since(UNIX_EPOCH).
            unwrap().
            as_secs() as i64;

        // Full query available in log_level DEBUG
        let sql_query = format!("INSERT INTO {} (time, pass) VALUES ($1, $2)", self.service_name);
        let rows_written = self.client.execute(sql_query.as_str(), &[&now, &pass]).await?;

        debug!(rows_written);
        Ok(())
    }
}


use sqlx::postgres::types::PgInterval;
use sqlx::types::chrono;

pub mod config;
pub mod db;
pub mod healthcheck;
pub mod webserver;

struct HealthcheckResult {
    check_id: i32,
    start_time: chrono::NaiveDateTime,
    elapsed_time: PgInterval,
    pass: bool,
    message: Option<String>,
}

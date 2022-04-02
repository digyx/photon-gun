mod basic;
mod luxury;

use std::time;

use tracing::error;

pub use self::basic::BasicCheck;
pub use self::luxury::LuxuryCheck;

pub struct HealthcheckResult {
    pub table_name: String,
    pub pass: bool,
    pub message: String,
    pub start_time: f64,
    pub elapsed_time: String,
}

impl HealthcheckResult {
    pub fn new(
        table_name: &str,
        pass: bool,
        message: String,
        start_time: time::SystemTime,
    ) -> HealthcheckResult {
        // Since UNIX_EPOCH is so far in the past, we can assume that duration_since won't fail
        let since_epoch = start_time.duration_since(time::UNIX_EPOCH).unwrap();
        // Elapsed time *could* go backwards, but it's highly unlikely
        let elapsed_time = match start_time.elapsed() {
            Ok(res) => res,
            Err(err) => {
                error!(service.name = %table_name, %err, msg = "TIME MOVED BACKWARDS; CLAMPING TO ZERO");
                time::Duration::ZERO
            }
        };

        HealthcheckResult {
            table_name: table_name.into(),
            pass,
            message,
            // Postgres's To_Timestamp function takes type "double precision"
            // https://www.postgresql.org/docs/current/functions-datetime.html
            start_time: since_epoch.as_secs_f64(),
            // We could measure in microseconds, but milliseconds are plenty enough precision
            // Postgres does not support microseconds
            // https://www.postgresql.org/docs/current/datatype-datetime.html#DATATYPE-INTERVAL-INPUT
            elapsed_time: format!("{} millisecond", elapsed_time.as_millis()),
        }
    }
}

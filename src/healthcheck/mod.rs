mod basic;
mod luxury;

use std::time;

use sqlx::postgres::types::PgInterval;
use sqlx::types::chrono;
use tracing::error;

use crate::HealthcheckResult;

pub use self::basic::BasicCheck;
pub use self::luxury::LuaCheck;

fn new_healthcheck_result(
    check_id: i32,
    pass: bool,
    message: Option<String>,
    start_time: time::SystemTime,
    elapsed_time: Result<time::Duration, time::SystemTimeError>,
) -> HealthcheckResult{
    // Since UNIX_EPOCH is so far in the past, we can assume that duration_since won't fail
    let since_epoch = start_time.duration_since(time::UNIX_EPOCH).unwrap();

    // Elapsed time *could* go backwards, but it's highly unlikely
    // I wanna handle this here instead of in the functions because less duplicate code is
    // better
    let elapsed_time = match elapsed_time {
        Ok(res) => res,
        Err(err) => {
            error!(%err, msg = "TIME MOVED BACKWARDS; CLAMPING TO ZERO");
            time::Duration::ZERO
        }
    };

    HealthcheckResult {
        check_id,
        start_time: chrono::NaiveDateTime::from_timestamp(since_epoch.as_secs() as i64, 0),
        elapsed_time: PgInterval{months: 0, days: 0, microseconds: elapsed_time.as_micros() as i64},
        pass,
        message,
    }
}

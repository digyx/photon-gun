use tonic::{Code, Status};
use tracing::error;

use crate::{query_filter, QueryFilter};

// Designed to be used with ? in gRPC handlers
pub(super) fn id_from_query_filter(filter: QueryFilter) -> Result<i32, Status> {
    let id = match filter.binds {
        Some(query_filter::Binds::Id(val)) => val,
        None => {
            error!(err = "no id given in request");
            return Err(Status::new(
                Code::InvalidArgument,
                "id is a required argument",
            ));
        }
    };

    Ok(id)
}

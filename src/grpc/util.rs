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

#[cfg(test)]
mod test {
    use super::*;
    use rstest::rstest;

    const VALID_ID_FILTER: QueryFilter = QueryFilter {
        binds: Some(query_filter::Binds::Id(1)),
    };

    const NONE_FILTER: QueryFilter = QueryFilter { binds: None };

    #[rstest]
    #[case(VALID_ID_FILTER)]
    #[should_panic] // None should return error
    #[case(NONE_FILTER)]
    fn test_id_from_query_filter(#[case] filter: QueryFilter) {
        id_from_query_filter(filter).unwrap();
    }
}

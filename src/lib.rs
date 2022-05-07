mod protobuf {
    tonic::include_proto!("photon_gun");
}

mod db;
pub mod grpc;
pub mod healthcheck;

pub use db::initialize_tables;
pub use healthcheck::load_from_database;

pub use protobuf::photon_gun_client::PhotonGunClient;
pub use protobuf::photon_gun_server::PhotonGunServer;
pub use protobuf::Empty;
pub use protobuf::{query_filter, ListQuery, QueryFilter, ResultQuery};
pub use protobuf::{
    Healthcheck, HealthcheckList, HealthcheckResult, HealthcheckResultList, HealthcheckUpdate,
};

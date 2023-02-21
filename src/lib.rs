mod protobuf {
    tonic::include_proto!("photon_gun");
}

mod db;
pub mod grpc;
pub mod healthcheck;

pub use db::initialize_tables;

pub use protobuf::photon_gun_client::PhotonGunClient;
pub use protobuf::photon_gun_server::PhotonGunServer;

pub use protobuf::{Healthcheck, PingRequest, PingResponse, Return};

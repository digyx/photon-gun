use std::fmt::Debug;

use clap::{Args, Parser, Subcommand};
use serde::Serialize;
use tonic::{Request, Response};

use photon_gun::{query_filter, Healthcheck, ListQuery, PhotonGunClient, QueryFilter, ResultQuery};

#[derive(Debug, Parser)]
struct ClapArgs {
    #[clap(long = "addr", short = 'h')]
    address: String,

    #[clap(subcommand)]
    action: Action,
}

#[derive(Debug, Subcommand)]
enum Action {
    Get(QueryOpts),
    List {
        #[clap(long = "disabled")]
        disabled: bool,
        limit: Option<i32>,
    },
    ListResults {
        id: i32,
        #[clap(long = "limit")]
        limit: Option<i32>,
    },
    Create {
        #[clap(long = "name")]
        name: Option<String>,
        endpoint: String,
        #[clap(long = "interval", default_value = "5")]
        interval: i32,
    },
    Delete(QueryOpts),
    Enable(QueryOpts),
    Disable(QueryOpts),
}

#[derive(Debug, Args)]
struct QueryOpts {
    id: i32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = ClapArgs::parse();
    let mut client = PhotonGunClient::connect(args.address).await?;

    match args.action {
        Action::Get(opts) => {
            let req = Request::new(QueryFilter {
                binds: Some(query_filter::Binds::Id(opts.id)),
            });

            let res = client.get_healthcheck(req).await?;
            print_response(res);
        }

        Action::List { disabled, limit } => {
            let req = Request::new(ListQuery {
                enabled: Some(!disabled),
                limit,
            });

            let res = client.list_healthchecks(req).await?;
            print_response(res);
        }

        Action::ListResults { id, limit } => {
            let req = Request::new(ResultQuery { id, limit });

            let res = client.list_healthcheck_results(req).await?;
            print_response(res);
        }

        Action::Create {
            name,
            endpoint,
            interval,
        } => {
            let req = Request::new(Healthcheck {
                id: 0, // This is overwritten by photon-server
                name,
                endpoint,
                interval,
                enabled: true,
            });

            let res = client.create_healthcheck(req).await?;
            print_response(res);
        }

        Action::Delete(opts) => {
            let req = Request::new(QueryFilter {
                binds: Some(query_filter::Binds::Id(opts.id)),
            });

            let res = client.delete_healthcheck(req).await?;
            print_response(res);
        }

        Action::Enable(opts) => {
            let req = Request::new(QueryFilter {
                binds: Some(query_filter::Binds::Id(opts.id)),
            });

            client.enable_healthcheck(req).await?;
            println!("Ok");
        }

        Action::Disable(opts) => {
            let req = Request::new(QueryFilter {
                binds: Some(query_filter::Binds::Id(opts.id)),
            });

            client.disable_healthcheck(req).await?;
            println!("Ok");
        }
    };

    Ok(())
}

fn print_response<T: Serialize>(res: Response<T>) {
    println!(
        "{}",
        serde_json::to_string_pretty(&res.into_inner()).unwrap()
    );
}

use std::thread::sleep;
use std::time::Duration;
use build_time::build_time_local;
use crate::provisioner::Provisioner;
use clap::{Args, Parser};
use clap::Subcommand;
use color_eyre::Result;
use crate::controller::Controller;

pub mod ext;
pub mod provisioner;
pub mod controller;
pub mod quantity_parser;
pub mod config;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Provision(ProvisionArgs)
}

#[derive(Args)]
struct ProvisionArgs {
    pvc_namespace: String,
    pvc_name: String,

    #[clap(env = "NODE_NAME", help = "The name of the Node the provisioner runs on")]
    node_name: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    println!("Running btrfs-provisioner built at {}", build_time_local!());

    let cli = Cli::parse();

    if let Some(command) = &cli.command {
        match command {
            Command::Provision(args) => {
                Provisioner::create()
                    .await?
                    .provision_persistent_volume_by_claim_name(
                        args.pvc_namespace.as_str(),
                        args.pvc_name.as_str(),
                        args.node_name.as_str(),
                    )
                    .await
            }
        }
    } else {
        Controller::create()
            .await?
            .run()
            .await
    }
}

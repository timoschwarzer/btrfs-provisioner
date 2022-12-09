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
pub mod btrfs_volume_metadata;
pub mod btrfs_wrapper;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Provision(ProvisionArgs),
    Delete(DeleteArgs)
}

#[derive(Args)]
struct ProvisionArgs {
    pvc_namespace: String,
    pvc_name: String,

    #[clap(env = "NODE_NAME", help = "The name of the Node the provisioner runs on")]
    node_name: String,
}

#[derive(Args)]
struct DeleteArgs {
    pv_name: String,

    #[clap(env = "NODE_NAME", help = "The name of the Node the provisioner runs on")]
    node_name: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    println!("Running btrfs-provisioner v{} built at {}", config::VERSION, build_time_local!());

    let cli = Cli::parse();

    if let Some(command) = &cli.command {
        match command {
            Command::Provision(args) => {
                Provisioner::create(args.node_name.to_owned())
                    .await?
                    .provision_persistent_volume_by_claim_name(
                        args.pvc_namespace.as_str(),
                        args.pvc_name.as_str(),
                    )
                    .await
            }
            Command::Delete(args) => {
                Provisioner::create(args.node_name.to_owned())
                    .await?
                    .delete_persistent_volume_by_name(args.pv_name.as_str())
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

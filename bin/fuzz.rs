//! TODO: git command
//!
//! example: https://github.com/clap-rs/clap/blob/master/examples/git-derive.rs

use anyhow::Result;
use clap::Parser;
use env_logger::Env;
use fuzz::service;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port of the fuzz service
    #[arg(short, long)]
    port: u16,
}

#[actix_web::main]
async fn main() -> Result<()> {
    let Args { port } = Args::parse();
    env_logger::try_init_from_env(Env::new().default_filter_or("fuzz=info"))?;
    service::start(port).await?;
    Ok(())
}

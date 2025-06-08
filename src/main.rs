mod agent;
mod client;
mod config;
mod metrics;

use clap::{Arg, Command};
use std::path::PathBuf;

use agent::SentinelAgent;
use config::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Command::new("Operion Sentinel Agent")
        .version("0.1.0")
        .about("Operion monitoring agent for system metrics")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Configuration file path")
                .default_value("/etc/operion/agent.yaml"),
        )
        .get_matches();

    let config_path = PathBuf::from(matches.get_one::<String>("config").unwrap());

    if !config_path.exists() {
        eprintln!("Configuration file not found: {}", config_path.display());
        eprintln!("Create a configuration file or specify a different path with --config");
        std::process::exit(1);
    }

    let config = Config::load_from_file(&config_path)?;
    let mut agent = SentinelAgent::new(config)?;
    agent.run().await?;

    Ok(())
}

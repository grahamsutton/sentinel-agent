mod agent;
mod client;
mod config;
mod metadata;
mod metrics;
mod state;

use clap::{Arg, Command};
use std::path::PathBuf;

use agent::SentinelAgent;
use config::Config;

fn find_default_config_path() -> PathBuf {
    // Priority order for config file locations:
    let candidates = vec![
        // 1. XDG config directory (industry standard, highest priority)
        dirs::home_dir().map(|dir| dir.join(".config").join("operion").join("agent.yaml")),
        // 2. Platform-specific config directory (fallback)
        dirs::config_dir().map(|dir| dir.join("operion").join("agent.yaml")),
        // 3. System-wide config
        Some(PathBuf::from("/etc/operion/agent.yaml")),
        // 4. Current directory (development/testing)
        Some(PathBuf::from("agent.yaml")),
    ];

    // Return the first config file that exists
    for candidate in candidates {
        if let Some(path) = candidate {
            if path.exists() {
                return path;
            }
        }
    }

    // If no config file exists, prefer XDG config directory
    if let Some(home_dir) = dirs::home_dir() {
        home_dir.join(".config").join("operion").join("agent.yaml")
    } else if let Some(config_dir) = dirs::config_dir() {
        config_dir.join("operion").join("agent.yaml")
    } else {
        // Fallback to system location
        PathBuf::from("/etc/operion/agent.yaml")
    }
}

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
                .help("Configuration file path (auto-detected if not specified)")
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .get_matches();

    let config_path = if let Some(config_path) = matches.get_one::<PathBuf>("config") {
        config_path.clone()
    } else {
        find_default_config_path()
    };

    if !config_path.exists() {
        eprintln!("Configuration file not found: {}", config_path.display());
        eprintln!("");
        eprintln!("Sentinel Agent looks for configuration files in this order:");
        if let Some(home_dir) = dirs::home_dir() {
            eprintln!("  1. {}", home_dir.join(".config").join("operion").join("agent.yaml").display());
        }
        if let Some(config_dir) = dirs::config_dir() {
            eprintln!("  2. {}", config_dir.join("operion").join("agent.yaml").display());
        }
        eprintln!("  3. /etc/operion/agent.yaml");
        eprintln!("  4. ./agent.yaml");
        eprintln!("");
        eprintln!("Create a configuration file in one of these locations, or specify a path with --config");
        std::process::exit(1);
    }

    let config = Config::load_from_file(&config_path)?;
    let mut agent = SentinelAgent::new(config)?;
    agent.run().await?;

    Ok(())
}

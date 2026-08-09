mod config;
mod launcher;

use std::{env, path::PathBuf, process::ExitCode};

use config::LauncherConfig;
use launcher::GameLauncher;

fn main() -> ExitCode {
    println!("PONTE MESH GAME LAUNCHER");
    println!("Local update delivery example\n");

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("\nUpdate failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let config_path = env::var_os("PONTEMESH_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("launcher.toml"));
    let config = LauncherConfig::load(&config_path)?;
    println!("Origin: {}", config.origin_url);
    println!("Object: {}/{}", config.bucket, config.object_key);
    println!("Downloading the configured update...\n");

    GameLauncher::new(config).install_update()?.print();
    Ok(())
}

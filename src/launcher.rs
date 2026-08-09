use std::{io::Write, path::PathBuf, time::Instant};

use pontemesh_sdk_core::{
    p2p::P2pConfig, PontemeshClient, PontemeshClientConfig, SyncObjectRequest, TransferSummary,
};

use crate::config::LauncherConfig;

pub struct GameLauncher {
    config: LauncherConfig,
}

impl GameLauncher {
    pub fn new(config: LauncherConfig) -> Self {
        Self { config }
    }

    pub fn install_update(&self) -> Result<InstallReport, String> {
        let client = PontemeshClient::new(PontemeshClientConfig {
            origin_url: self.config.origin_url.clone(),
            application_token: self.config.application_token.clone(),
            p2p: P2pConfig::default(),
        })
        .map_err(|error| error.to_string())?;

        let destination = PathBuf::from(&self.config.destination);
        let started = Instant::now();
        let mut progress = |fragment: u32, downloaded: u64, total: u64, source: &str| {
            print!(
                "\rFragment {:>3}: {:>8} / {:>8} bytes via {:<12}",
                fragment + 1,
                downloaded,
                total,
                source
            );
            let _ = std::io::stdout().flush();
        };

        let result = client
            .sync_object_with_summary_and_progress(
                SyncObjectRequest {
                    bucket: self.config.bucket.clone(),
                    key: self.config.object_key.clone(),
                    destination: destination.clone(),
                },
                Some(&mut progress),
            )
            .map_err(|error| error.to_string())?;

        println!();
        Ok(InstallReport {
            destination,
            bytes: result.bytes.len() as u64,
            elapsed_ms: started.elapsed().as_millis(),
            summary: result.summary,
        })
    }
}

pub struct InstallReport {
    pub destination: PathBuf,
    pub bytes: u64,
    pub elapsed_ms: u128,
    pub summary: TransferSummary,
}

impl InstallReport {
    pub fn print(&self) {
        println!("\nUpdate installed successfully.");
        println!("  File: {}", self.destination.display());
        println!("  Size: {} bytes", self.bytes);
        println!("  Time: {} ms", self.elapsed_ms);
        println!("  Origin: {} bytes", self.summary.bytes_from_origin);
        println!("  Replica/Edge: {} bytes", self.summary.bytes_from_replica);
        println!("  Peers: {} bytes", self.summary.bytes_from_peer);
        println!("\nGame status: READY TO PLAY");
    }
}

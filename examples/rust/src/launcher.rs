use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use pontemesh_sdk_core::{
    integrity::sha256_hex,
    p2p::{P2pConfig, P2pTransportKind},
    release::ReleaseManifest,
    CancellationToken, PontemeshClient, PontemeshClientConfig, SyncObjectRequest, TransferSummary,
};

use crate::config::LauncherConfig;

const MAX_RELEASE_SIZE_BYTES: u64 = 20 * 1024 * 1024 * 1024;
const MAX_RELEASE_FILES: usize = 10_000;
const INSTALL_OVERHEAD_BYTES: u64 = 16 * 1024 * 1024;

pub struct GameLauncher {
    config: LauncherConfig,
}

impl GameLauncher {
    pub fn new(config: LauncherConfig) -> Self {
        Self { config }
    }

    pub fn install_latest(&self, cancellation: CancellationToken) -> Result<InstallReport, String> {
        let client = self.client()?;
        let install_root = PathBuf::from(&self.config.install_directory);
        let parent = install_root
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let release_work = tempfile::Builder::new()
            .prefix(".pontemesh-release-")
            .tempdir_in(parent)
            .map_err(|error| error.to_string())?;
        let descriptor_path = release_work.path().join("release.json");
        let fragment_cache = install_root.with_extension("pontemesh-cache");

        client
            .sync_object_to_disk_with_cache(
                SyncObjectRequest {
                    bucket: self.config.release_bucket.clone(),
                    key: self.config.release_manifest_key.clone(),
                    destination: descriptor_path.clone(),
                },
                fragment_cache.clone(),
                None,
                cancellation.clone(),
            )
            .map_err(|error| error.to_string())?;

        let manifest = ReleaseManifest::from_json(
            &fs::read(&descriptor_path).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let required = required_free_space(manifest.total_size_bytes(), manifest.files.len())?;
        let available = fs2::available_space(parent).map_err(|error| error.to_string())?;
        if available < required {
            return Err(format!(
                "Not enough disk space: {required} bytes required for the cache and staged installation, {available} available"
            ));
        }

        let staging = release_work.path().join("installation");
        fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
        let started = Instant::now();
        let mut summary = TransferSummary::default();

        for file in manifest.files_in_install_order() {
            let destination = staging.join(&file.path);
            let label = file.path.clone();
            let mut progress = |fragment: u32, downloaded: u64, total: u64, source: &str| {
                let percent = downloaded
                    .saturating_mul(100)
                    .checked_div(total)
                    .unwrap_or(0);
                println!(
                    "{label}: {percent:>3}% ({downloaded}/{total} bytes, fragment {}, {source})",
                    fragment + 1
                );
            };
            let file_summary = client
                .sync_object_to_disk_with_cache(
                    SyncObjectRequest {
                        bucket: file.bucket.clone(),
                        key: file.key.clone(),
                        destination: destination.clone(),
                    },
                    fragment_cache.clone(),
                    Some(&mut progress),
                    cancellation.clone(),
                )
                .map_err(|error| error.to_string())?;
            let bytes = fs::read(&destination).map_err(|error| error.to_string())?;
            if bytes.len() as u64 != file.size_bytes
                || !sha256_hex(&bytes).eq_ignore_ascii_case(&file.sha256)
            {
                return Err(format!("Release verification failed for {}", file.path));
            }
            merge_summary(&mut summary, &file_summary);
        }

        fs::write(
            staging.join(".pontemesh-version"),
            format!("{}\n", manifest.version),
        )
        .map_err(|error| error.to_string())?;
        replace_installation(&install_root, &staging)?;
        if self.config.seed_seconds > 0 {
            println!(
                "Keeping validated fragments available to LAN peers for {} seconds...",
                self.config.seed_seconds
            );
            for _ in 0..self.config.seed_seconds {
                if cancellation.is_cancelled() {
                    break;
                }
                thread::sleep(Duration::from_secs(1));
            }
        }

        let total_size = manifest.total_size_bytes();
        Ok(InstallReport {
            destination: install_root,
            version: manifest.version,
            files: manifest.files.len(),
            bytes: total_size,
            elapsed_ms: started.elapsed().as_millis(),
            summary,
        })
    }

    fn client(&self) -> Result<PontemeshClient, String> {
        let p2p = match &self.config.p2p_listen_address {
            Some(listen) => P2pConfig {
                enabled: true,
                required: false,
                transport: P2pTransportKind::Libp2p,
                listen_addrs: vec![listen.clone()],
                announce_addrs: self
                    .config
                    .p2p_announce_address
                    .clone()
                    .into_iter()
                    .collect(),
                listen_addr: None,
                announce_addr: None,
            },
            None => P2pConfig::default(),
        };
        PontemeshClient::new(PontemeshClientConfig {
            origin_url: self.config.origin_url.clone(),
            application_token: self.config.application_token.clone(),
            p2p,
        })
        .map_err(|error| error.to_string())
    }
}

fn required_free_space(total_size: u64, file_count: usize) -> Result<u64, String> {
    if file_count > MAX_RELEASE_FILES {
        return Err(format!(
            "Release contains {file_count} files; the maximum is {MAX_RELEASE_FILES}"
        ));
    }
    if total_size > MAX_RELEASE_SIZE_BYTES {
        return Err(format!(
            "Release is {total_size} bytes; the maximum is {MAX_RELEASE_SIZE_BYTES}"
        ));
    }
    total_size
        .checked_mul(2)
        .and_then(|size| size.checked_add(INSTALL_OVERHEAD_BYTES))
        .ok_or_else(|| "Release storage requirement overflowed".to_owned())
}

fn replace_installation(install_root: &Path, staging: &Path) -> Result<(), String> {
    let rollback = install_root.with_extension("pontemesh-rollback");
    if rollback.exists() {
        fs::remove_dir_all(&rollback).map_err(|error| error.to_string())?;
    }
    if install_root.exists() {
        fs::rename(install_root, &rollback).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(staging, install_root) {
        if rollback.exists() {
            fs::rename(&rollback, install_root).map_err(|restore| {
                format!("Installation failed ({error}) and rollback failed ({restore})")
            })?;
        }
        return Err(format!("Installation failed: {error}"));
    }
    if rollback.exists() {
        fs::remove_dir_all(rollback).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn merge_summary(total: &mut TransferSummary, next: &TransferSummary) {
    total.bytes_from_peer += next.bytes_from_peer;
    total.bytes_from_replica += next.bytes_from_replica;
    total.bytes_from_origin += next.bytes_from_origin;
    total.fragments_from_peer += next.fragments_from_peer;
    total.fragments_from_replica += next.fragments_from_replica;
    total.fragments_from_origin += next.fragments_from_origin;
    total.peer_failures += next.peer_failures;
    total.peer_hash_failures += next.peer_hash_failures;
    total.peer_rejected_fragments += next.peer_rejected_fragments;
    total.fallback_activations += next.fallback_activations;
}

pub struct InstallReport {
    pub destination: PathBuf,
    pub version: String,
    pub files: usize,
    pub bytes: u64,
    pub elapsed_ms: u128,
    pub summary: TransferSummary,
}

impl InstallReport {
    pub fn print(&self) {
        println!("\nUpdate {} installed successfully.", self.version);
        println!("  Directory: {}", self.destination.display());
        println!("  Files: {}", self.files);
        println!("  Size: {} bytes", self.bytes);
        println!("  Time: {} ms", self.elapsed_ms);
        println!("  Origin: {} bytes", self.summary.bytes_from_origin);
        println!("  Replica/Edge: {} bytes", self.summary.bytes_from_replica);
        println!("  Peers: {} bytes", self.summary.bytes_from_peer);
        println!("\nGame status: READY TO PLAY");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserves_space_for_cache_staging_and_overhead() {
        assert_eq!(
            required_free_space(8 * 1024 * 1024, 3).expect("release should fit"),
            32 * 1024 * 1024
        );
    }

    #[test]
    fn rejects_unbounded_release_descriptors() {
        assert!(required_free_space(MAX_RELEASE_SIZE_BYTES + 1, 1).is_err());
        assert!(required_free_space(1, MAX_RELEASE_FILES + 1).is_err());
    }
}

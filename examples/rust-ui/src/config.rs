use std::{env, fs, path::{Path, PathBuf}};

use reqwest::Url;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LauncherConfig {
    #[serde(default = "default_origin_url")]
    pub origin_url: String,
    #[serde(default)]
    pub application_token: String,
    #[serde(default = "default_release_bucket")]
    pub release_bucket: String,
    #[serde(default = "default_release_manifest_key")]
    pub release_manifest_key: String,
    #[serde(default = "default_install_directory")]
    pub install_directory: String,
    #[serde(default = "default_cache_directory")]
    pub cache_directory: String,
    #[serde(default)]
    pub p2p_listen_address: Option<String>,
    #[serde(default)]
    pub p2p_announce_address: Option<String>,
    #[serde(default)]
    pub seed_seconds: u64,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            origin_url: default_origin_url(),
            application_token: String::new(),
            release_bucket: default_release_bucket(),
            release_manifest_key: default_release_manifest_key(),
            install_directory: default_install_directory(),
            cache_directory: default_cache_directory(),
            p2p_listen_address: Some("/ip4/0.0.0.0/tcp/9095".to_string()),
            p2p_announce_address: None,
            seed_seconds: 0,
        }
    }
}

impl LauncherConfig {
    pub fn load(path: &Path) -> Result<Self, String> {
        let _ = dotenvy::dotenv();
        let _ = dotenvy::from_filename(".env");
        let _ = dotenvy::from_filename("../.env");
        let _ = dotenvy::from_filename("../../.env");

        let mut config = if path.exists() {
            let contents = fs::read_to_string(path).map_err(|error| {
                format!("Could not read {}: {error}", path.display())
            })?;
            toml::from_str(&contents).map_err(|error| format!("Invalid {}: {error}", path.display()))?
        } else {
            Self::default()
        };

        if let Ok(value) = env::var("PONTEMESH_ORIGIN_URL") {
            if !value.trim().is_empty() {
                config.origin_url = value;
            }
        }
        if let Ok(value) = env::var("PONTEMESH_APPLICATION_TOKEN") {
            if !value.trim().is_empty() {
                config.application_token = value;
            }
        }
        if let Ok(value) = env::var("PONTEMESH_RELEASE_BUCKET") {
            if !value.trim().is_empty() {
                config.release_bucket = value;
            }
        }
        if let Ok(value) = env::var("PONTEMESH_RELEASE_MANIFEST_KEY") {
            if !value.trim().is_empty() {
                config.release_manifest_key = value;
            }
        }
        if let Ok(value) = env::var("PONTEMESH_INSTALL_DIRECTORY") {
            if !value.trim().is_empty() {
                config.install_directory = value;
            }
        }
        if let Ok(value) = env::var("PONTEMESH_CACHE_DIRECTORY") {
            if !value.trim().is_empty() {
                config.cache_directory = value;
            }
        }
        if let Ok(value) = env::var("PONTEMESH_P2P_LISTEN_ADDRESS") {
            if !value.trim().is_empty() {
                config.p2p_listen_address = Some(value);
            }
        }
        if let Ok(value) = env::var("PONTEMESH_P2P_ANNOUNCE_ADDRESS") {
            if !value.trim().is_empty() {
                config.p2p_announce_address = Some(value);
            }
        }
        if let Ok(value) = env::var("PONTEMESH_SEED_SECONDS") {
            if let Ok(parsed) = value.parse::<u64>() {
                config.seed_seconds = parsed;
            }
        }

        if config.cache_directory.trim().is_empty() {
            config.cache_directory = format!("{}.pontemesh-cache", config.install_directory);
        }

        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let serialized = toml::to_string_pretty(self)
            .map_err(|error| format!("Failed to serialize config: {error}"))?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("Failed to create config directory: {error}"))?;
            }
        }
        fs::write(path, serialized)
            .map_err(|error| format!("Failed to write {}: {error}", path.display()))?;
        Ok(())
    }

    pub fn resolved_cache_path(&self) -> PathBuf {
        if self.cache_directory.trim().is_empty() {
            PathBuf::from(&self.install_directory).with_extension("pontemesh-cache")
        } else {
            PathBuf::from(&self.cache_directory)
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let origin = Url::parse(&self.origin_url)
            .map_err(|error| format!("origin_url must be a valid URL: {error}"))?;
        match origin.scheme() {
            "http" | "https" => {}
            _ => return Err("origin_url must use http:// or https://".to_owned()),
        }
        if origin.host_str().is_none() {
            return Err("origin_url must include a host".to_owned());
        }
        for (name, value) in [
            ("application_token", self.application_token.as_str()),
            ("release_bucket", self.release_bucket.as_str()),
            ("release_manifest_key", self.release_manifest_key.as_str()),
            ("install_directory", self.install_directory.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("{name} cannot be empty"));
            }
        }
        if self.application_token == "set-by-bootstrap-or-environment" {
            return Err("Set PONTEMESH_APPLICATION_TOKEN or enter a valid application token".to_owned());
        }
        Ok(())
    }
}

fn default_origin_url() -> String {
    "http://127.0.0.1:8080".to_owned()
}

fn default_release_bucket() -> String {
    "game-updates".to_owned()
}

fn default_release_manifest_key() -> String {
    "releases/stable.json".to_owned()
}

fn default_install_directory() -> String {
    "runtime/installations/rust".to_owned()
}

fn default_cache_directory() -> String {
    "runtime/installations/rust.pontemesh-cache".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_documented_configuration() {
        let config: LauncherConfig = toml::from_str(
            r#"
application_token = "pm_app_example"
release_bucket = "game-updates"
"#,
        )
        .expect("configuration should parse");

        assert_eq!(config.origin_url, "http://127.0.0.1:8080");
        assert_eq!(config.release_manifest_key, "releases/stable.json");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn rejects_the_documented_token_placeholder() {
        let mut config = valid_config();
        config.application_token = "set-by-bootstrap-or-environment".to_owned();
        assert!(config.validate().is_err());
    }

    #[test]
    fn accepts_http_origins_on_the_local_network() {
        let mut config = valid_config();
        config.origin_url = "http://192.168.1.20:8080".to_owned();
        assert!(config.validate().is_ok());

        config.origin_url = "http://game-origin.local:8080".to_owned();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn accepts_https_remote_origins() {
        let mut config = valid_config();
        config.origin_url = "https://134.65.234.41:8080".to_owned();
        assert!(config.validate().is_ok());

        config.origin_url = "https://updates.example.com".to_owned();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn rejects_unsupported_or_incomplete_origin_urls() {
        let mut config = valid_config();
        config.origin_url = "ftp://192.168.1.20/releases".to_owned();
        assert!(config.validate().is_err());

        config.origin_url = "not a URL".to_owned();
        assert!(config.validate().is_err());
    }

    fn valid_config() -> LauncherConfig {
        LauncherConfig {
            origin_url: default_origin_url(),
            application_token: "pm_app_example".to_owned(),
            release_bucket: default_release_bucket(),
            release_manifest_key: default_release_manifest_key(),
            install_directory: default_install_directory(),
            cache_directory: default_cache_directory(),
            p2p_listen_address: None,
            p2p_announce_address: None,
            seed_seconds: 0,
        }
    }
}

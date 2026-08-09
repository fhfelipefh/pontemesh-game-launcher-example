use std::{env, fs, path::Path};

use reqwest::Url;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct LauncherConfig {
    #[serde(default = "default_origin_url")]
    pub origin_url: String,
    #[serde(default)]
    pub application_token: String,
    pub release_bucket: String,
    #[serde(default = "default_release_manifest_key")]
    pub release_manifest_key: String,
    #[serde(default = "default_install_directory")]
    pub install_directory: String,
    #[serde(default)]
    pub p2p_listen_address: Option<String>,
    #[serde(default)]
    pub p2p_announce_address: Option<String>,
    #[serde(default)]
    pub seed_seconds: u64,
}

impl LauncherConfig {
    pub fn load(path: &Path) -> Result<Self, String> {
        let contents = fs::read_to_string(path).map_err(|error| {
            format!(
                "Could not read {}: {error}. Copy launcher.example.toml to launcher.toml first.",
                path.display()
            )
        })?;
        let mut config: Self = toml::from_str(&contents)
            .map_err(|error| format!("Invalid {}: {error}", path.display()))?;

        if let Ok(value) = env::var("PONTEMESH_ORIGIN_URL") {
            config.origin_url = value;
        }
        if let Ok(value) = env::var("PONTEMESH_APPLICATION_TOKEN") {
            config.application_token = value;
        }

        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        let origin = Url::parse(&self.origin_url)
            .map_err(|error| format!("origin_url must be a valid URL: {error}"))?;
        match origin.scheme() {
            "https" => {}
            "http" if is_loopback_origin(&origin) => {}
            "http" => {
                return Err(
                    "origin_url must use HTTPS unless it points to the local computer".to_owned(),
                )
            }
            _ => return Err("origin_url must use http:// or https://".to_owned()),
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
            return Err(
                "Set PONTEMESH_APPLICATION_TOKEN or update the ignored launcher.toml".to_owned(),
            );
        }
        Ok(())
    }
}

fn is_loopback_origin(origin: &Url) -> bool {
    origin.host_str().is_some_and(|host| {
        let host = host.trim_start_matches('[').trim_end_matches(']');
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    })
}

fn default_origin_url() -> String {
    "http://127.0.0.1:8080".to_owned()
}

fn default_release_manifest_key() -> String {
    "releases/stable.json".to_owned()
}

fn default_install_directory() -> String {
    "runtime/installed-game".to_owned()
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
        let config = LauncherConfig {
            origin_url: default_origin_url(),
            application_token: "set-by-bootstrap-or-environment".to_owned(),
            release_bucket: "game-updates".to_owned(),
            release_manifest_key: default_release_manifest_key(),
            install_directory: default_install_directory(),
            p2p_listen_address: None,
            p2p_announce_address: None,
            seed_seconds: 0,
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn accepts_plain_http_only_for_loopback_origins() {
        let mut config = valid_config();
        config.origin_url = "http://localhost:8080".to_owned();
        assert!(config.validate().is_ok());

        config.origin_url = "http://[::1]:8080".to_owned();
        assert!(config.validate().is_ok());

        config.origin_url = "http://192.168.1.20:8080".to_owned();
        assert!(config.validate().is_err());
    }

    fn valid_config() -> LauncherConfig {
        LauncherConfig {
            origin_url: default_origin_url(),
            application_token: "pm_app_example".to_owned(),
            release_bucket: "game-updates".to_owned(),
            release_manifest_key: default_release_manifest_key(),
            install_directory: default_install_directory(),
            p2p_listen_address: None,
            p2p_announce_address: None,
            seed_seconds: 0,
        }
    }
}

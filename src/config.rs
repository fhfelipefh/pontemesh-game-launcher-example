use std::{env, fs, path::Path};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct LauncherConfig {
    #[serde(default = "default_origin_url")]
    pub origin_url: String,
    pub application_token: String,
    pub bucket: String,
    pub object_key: String,
    pub destination: String,
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
        if !(self.origin_url.starts_with("http://") || self.origin_url.starts_with("https://")) {
            return Err("origin_url must start with http:// or https://".to_owned());
        }

        for (name, value) in [
            ("application_token", self.application_token.as_str()),
            ("bucket", self.bucket.as_str()),
            ("object_key", self.object_key.as_str()),
            ("destination", self.destination.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("{name} cannot be empty"));
            }
        }

        if self.application_token == "paste-the-token-created-in-the-server-panel" {
            return Err("Replace the placeholder application_token in launcher.toml".to_owned());
        }

        Ok(())
    }
}

fn default_origin_url() -> String {
    "http://127.0.0.1:8080".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_documented_configuration() {
        let config: LauncherConfig = toml::from_str(
            r#"
application_token = "pm_app_example"
bucket = "game-updates"
object_key = "releases/1.0.0/game-update.pak"
destination = "installed-game/game-update.pak"
"#,
        )
        .expect("configuration should parse");

        assert_eq!(config.origin_url, "http://127.0.0.1:8080");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn rejects_the_documented_token_placeholder() {
        let config = LauncherConfig {
            origin_url: default_origin_url(),
            application_token: "paste-the-token-created-in-the-server-panel".to_owned(),
            bucket: "game-updates".to_owned(),
            object_key: "update.pak".to_owned(),
            destination: "installed-game/update.pak".to_owned(),
        };

        assert_eq!(
            config.validate(),
            Err("Replace the placeholder application_token in launcher.toml".to_owned())
        );
    }
}

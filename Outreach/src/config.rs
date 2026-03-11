use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Application-wide configuration, loaded from ~/.config/outreach/config.toml
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    pub auth: AuthConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Azure AD app client ID (public client, no secret needed)
    pub client_id: String,
    /// Tenant ID or "common" for multi-tenant
    pub tenant_id: String,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            // Placeholder — users must register their own Azure app
            client_id: String::from("YOUR_CLIENT_ID"),
            tenant_id: String::from("common"),
        }
    }
}

impl Config {
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("outreach")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn token_path() -> PathBuf {
        Self::config_dir().join("token.json")
    }

    pub fn db_path() -> PathBuf {
        Self::config_dir().join("outreach.db")
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)?;
        let content = toml::to_string_pretty(self)?;
        std::fs::write(Self::config_path(), content)?;
        Ok(())
    }
}

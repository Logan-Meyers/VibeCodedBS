use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub general: GeneralConfig,
    pub keybinds: KeybindConfig,
    pub conversion: ConversionConfig,
    pub metadata: MetadataConfig,
    pub providers: ProvidersConfig,
    pub export: ExportConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub working_copy_dir: String,
    pub source_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindConfig {
    pub navigate_up: char,
    pub navigate_down: char,
    pub expand: char,
    pub collapse: char,
    pub quit: char,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionConfig {
    pub enabled: bool,
    pub format: String,
    pub bitrate: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataConfig {
    pub essentials: Vec<String>,
    pub provider_order: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidersConfig {
    pub tidal: TidalConfig,
    pub soundcloud: SoundcloudConfig,
    pub musicbrainz: MusicBrainzConfig,
    pub itunes: ItunesConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TidalConfig {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoundcloudConfig {
    pub client_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicBrainzConfig {
    pub user_agent: String,
    pub rate_limit_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItunesConfig {
    pub rate_limit_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    pub rockbox_layout: String,
    pub art_format: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                working_copy_dir: "~/musicman_working".into(),
                source_dir: "~/Music".into(),
            },
            keybinds: KeybindConfig {
                navigate_up: 'k',
                navigate_down: 'j',
                expand: 'l',
                collapse: 'h',
                quit: 'q',
            },
            conversion: ConversionConfig {
                enabled: false,
                format: "m4a".into(),
                bitrate: "320k".into(),
            },
            metadata: MetadataConfig {
                essentials: vec![
                    "title".into(),
                    "artist".into(),
                    "track_number".into(),
                    "year".into(),
                ],
                provider_order: vec![
                    "musicbrainz".into(),
                    "itunes".into(),
                    "tidal".into(),
                    "soundcloud".into(),
                ],
            },
            providers: ProvidersConfig {
                tidal: TidalConfig {
                    client_id: String::new(),
                    client_secret: String::new(),
                },
                soundcloud: SoundcloudConfig {
                    client_id: String::new(),
                },
                musicbrainz: MusicBrainzConfig {
                    user_agent: "musicman/0.1.0 (user@example.com)".into(),
                    rate_limit_ms: 1100,
                },
                itunes: ItunesConfig {
                    rate_limit_ms: 500,
                },
            },
            export: ExportConfig {
                rockbox_layout: "Music/{artist}/{album}/{track_number} - {title}".into(),
                art_format: "bmp".into(),
            },
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config at {}", path.display()))?;
        let config: Config = toml::from_str(&content)
            .with_context(|| "Failed to parse config.toml")?;
        Ok(config)
    }

    pub fn load_or_default() -> Result<Self> {
        let config_path = config_path();
        if config_path.exists() {
            Self::load(&config_path)
        } else {
            let config = Self::default();
            config.save(&config_path)?;
            Ok(config)
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Resolve ~ in paths
    pub fn resolve_source_dir(&self) -> PathBuf {
        resolve_tilde(&self.general.source_dir)
    }

    pub fn resolve_working_dir(&self) -> PathBuf {
        resolve_tilde(&self.general.working_copy_dir)
    }
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("musicman")
        .join("config.toml")
}

fn resolve_tilde(path: &str) -> PathBuf {
    if path.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            return home.join(&path[2..]);
        }
    }
    PathBuf::from(path)
}

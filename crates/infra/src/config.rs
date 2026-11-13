use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetadataProviderKind {
    #[default]
    Tmdb,
    Omdb,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub provider: MetadataProviderKind,
    pub tmdb_api_key: String,
    pub tmdb_read_token: String,
    pub omdb_api_key: String,
    pub language: String,
    pub cache_ttl_days: u32,
    pub offline: bool,
    pub auto_accept_threshold: f32,
    pub review_threshold: f32,
    pub concurrency: usize,
    pub requests_per_second: u32,
    pub template: String,
    pub tv_template: String,
    pub series_folder_template: String,
    pub create_movie_folders: bool,
    pub create_tv_folders: bool,
    pub move_sidecars: bool,
    pub max_component_len: usize,
    pub ffprobe_path: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: MetadataProviderKind::Tmdb,
            tmdb_api_key: String::new(),
            tmdb_read_token: String::new(),
            omdb_api_key: String::new(),
            language: "en-US".to_string(),
            cache_ttl_days: 30,
            offline: false,
            auto_accept_threshold: 0.85,
            review_threshold: 0.55,
            concurrency: 8,
            requests_per_second: 10,
            template: "{title} ({year}) [tmdbid-{id}]".to_string(),
            tv_template: "{title} ({year}) S{season}E{episode} - {episode_title}".to_string(),
            series_folder_template: "{title} ({year}) [tmdbid-{id}]".to_string(),
            create_movie_folders: true,
            create_tv_folders: true,
            move_sidecars: true,
            max_component_len: 180,
            ffprobe_path: None,
        }
    }
}

impl Config {
    pub fn load_or_default(path: &Path) -> Result<Self, ConfigError> {
        let mut config = if path.exists() {
            let text = std::fs::read_to_string(path)?;
            toml::from_str(&text)?
        } else {
            Self::default()
        };
        config.apply_env();
        config.validate();
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        std::fs::write(path, text)?;
        Ok(())
    }

    pub fn apply_env(&mut self) {
        if let Ok(value) = std::env::var("TMDB_API_KEY") {
            if !value.is_empty() {
                self.tmdb_api_key = value;
            }
        }
        if let Ok(value) = std::env::var("TMDB_READ_TOKEN") {
            if !value.is_empty() {
                self.tmdb_read_token = value;
            }
        }
        if let Ok(value) = std::env::var("OMDB_API_KEY") {
            if !value.is_empty() {
                self.omdb_api_key = value;
            }
        }
        if let Ok(value) = std::env::var("MEDIAPARSE_OFFLINE") {
            self.offline = matches!(value.as_str(), "1" | "true" | "TRUE" | "yes");
        }
        if let Ok(value) = std::env::var("MEDIAPARSE_PROVIDER") {
            self.provider = match value.to_ascii_lowercase().as_str() {
                "omdb" => MetadataProviderKind::Omdb,
                _ => MetadataProviderKind::Tmdb,
            };
        }
        if let Ok(value) = std::env::var("MEDIAPARSE_LANGUAGE") {
            if !value.is_empty() {
                self.language = value;
            }
        }
    }

    pub fn validate(&mut self) {
        self.concurrency = self.concurrency.clamp(1, 16);
        self.requests_per_second = self.requests_per_second.clamp(1, 50);
        self.cache_ttl_days = self.cache_ttl_days.max(1);
        self.max_component_len = self.max_component_len.clamp(32, 240);
        if self.review_threshold <= 0.0 || self.review_threshold >= 1.0 {
            self.review_threshold = 0.55;
        }
        if self.auto_accept_threshold <= self.review_threshold || self.auto_accept_threshold > 1.0 {
            self.auto_accept_threshold = (self.review_threshold + 0.30).min(0.99);
        }
        if self.template.trim().is_empty() {
            self.template = Config::default().template;
        }
        if self.tv_template.trim().is_empty() {
            self.tv_template = Config::default().tv_template;
        }
        if self.series_folder_template.trim().is_empty() {
            self.series_folder_template = Config::default().series_folder_template;
        }
        if self.language.trim().is_empty() {
            self.language = "en-US".to_string();
        }
    }

    pub fn has_tmdb_credentials(&self) -> bool {
        !self.tmdb_read_token.trim().is_empty() || !self.tmdb_api_key.trim().is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub config_file: PathBuf,
    pub database_file: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mediaparse");
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mediaparse");
        Self::from_dirs(config_dir, data_dir)
    }

    pub fn from_dirs(config_dir: PathBuf, data_dir: PathBuf) -> Self {
        Self {
            config_file: config_dir.join("config.toml"),
            database_file: data_dir.join("mediaparse.sqlite3"),
            config_dir,
            data_dir,
        }
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.config_dir)?;
        std::fs::create_dir_all(&self.data_dir)?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),
    #[error("TOML serialization error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let mut config = Config::default();
        config.validate();
        assert_eq!(config.concurrency, 8);
        assert!(config.auto_accept_threshold > config.review_threshold);
    }

    #[test]
    fn clamps_bad_values() {
        let mut config = Config {
            concurrency: 99,
            requests_per_second: 0,
            max_component_len: 1,
            review_threshold: 0.9,
            auto_accept_threshold: 0.2,
            ..Config::default()
        };
        config.validate();
        assert_eq!(config.concurrency, 16);
        assert_eq!(config.requests_per_second, 1);
        assert_eq!(config.max_component_len, 32);
        assert!(config.auto_accept_threshold > config.review_threshold);
    }

    #[test]
    fn round_trips_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let config = Config {
            template: "{title} ({year})".into(),
            ..Config::default()
        };
        config.save(&path).unwrap();
        let loaded = Config::load_or_default(&path).unwrap();
        assert_eq!(loaded.template, "{title} ({year})");
    }

    #[test]
    fn missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::load_or_default(&dir.path().join("nope.toml")).unwrap();
        assert_eq!(config.provider, MetadataProviderKind::Tmdb);
    }
}

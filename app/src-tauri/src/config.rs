use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Manager;
use tracing::{error, info, warn};

pub const MIN_DROP_OPACITY: u8 = 20;
pub const DEFAULT_LANGUAGE: &str = "en";

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum DropSize {
    Medium,
    Large,
    #[default]
    #[serde(other)]
    Small,
}

impl DropSize {
    pub const fn scale(self) -> f64 {
        match self {
            Self::Small => 1.0,
            Self::Medium => 1.2,
            Self::Large => 1.5,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RuntimeDropSize(pub DropSize);

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub mouse_monitor: MouseMonitorConfig,
    pub autostart: bool,
    pub hotkey: String,
    #[serde(default = "default_drop_opacity")]
    pub drop_opacity: u8,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub drop_size: DropSize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MouseMonitorConfig {
    pub required_shakes: u32,
    pub shake_time_limit: u64,
    pub shake_threshold: i32,
    #[serde(default)]
    pub blacklist: Vec<String>,
}

fn default_drop_opacity() -> u8 {
    88
}

fn default_language() -> String {
    DEFAULT_LANGUAGE.to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            mouse_monitor: MouseMonitorConfig {
                required_shakes: 5,
                shake_time_limit: 1500,
                shake_threshold: 100,
                blacklist: Vec::new(),
            },
            autostart: false,
            hotkey: "".to_string(),
            drop_opacity: default_drop_opacity(),
            language: default_language(),
            drop_size: DropSize::default(),
        }
    }
}

impl AppConfig {
    pub fn load(app_handle: &AppHandle) -> Self {
        let config_path = match Self::get_config_path(app_handle) {
            Ok(path) => path,
            Err(e) => {
                error!("Failed to get config path: {}", e);
                warn!("Using default config due to config path error");
                return Self::default();
            }
        };

        info!("Loading config from {:?}", config_path);

        if let Ok(contents) = fs::read_to_string(&config_path) {
            match serde_json::from_str::<AppConfig>(&contents) {
                Ok(mut config) => {
                    config.drop_opacity = config.drop_opacity.clamp(MIN_DROP_OPACITY, 100);
                    if !matches!(config.language.as_str(), "en" | "vi") {
                        config.language = default_language();
                    }
                    return config;
                }
                Err(e) => {
                    error!("Failed to parse config file: {}", e);
                    warn!("Using default config due to config parse error");
                }
            }
        } else {
            warn!("Config file not found or unreadable");
        }

        // If loading fails, return default config
        info!("Using default config");
        Self::default()
    }

    pub fn save(&self, app_handle: &AppHandle) -> Result<(), String> {
        let config_path = Self::get_config_path(app_handle)?;
        info!("Saving config to {:?}", config_path);

        let config_dir = config_path.parent().ok_or("Invalid config path")?;

        // Create config directory if it doesn't exist
        if !config_dir.exists() {
            info!("Creating config directory at {:?}", config_dir);
            fs::create_dir_all(config_dir)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        let contents = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        fs::write(&config_path, contents)
            .map_err(|e| format!("Failed to write config file: {}", e))?;

        info!("Config saved successfully");
        Ok(())
    }

    fn get_config_path(app_handle: &AppHandle) -> Result<PathBuf, String> {
        let app_dir = app_handle
            .path()
            .app_config_dir()
            .map_err(|e| format!("Failed to get app config directory: {}", e))?;
        let config_path = app_dir.join("config.json");
        Ok(config_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_config_uses_new_field_defaults() {
        let json = r#"{
            "mouse_monitor": {
                "required_shakes": 5,
                "shake_time_limit": 1500,
                "shake_threshold": 100,
                "whitelist": ["explorer.exe"]
            },
            "autostart": false,
            "hotkey": ""
        }"#;

        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.drop_opacity, 88);
        assert_eq!(config.language, "en");
        assert_eq!(config.drop_size, DropSize::Small);
        assert!(config.mouse_monitor.blacklist.is_empty());
    }

    #[test]
    fn invalid_drop_size_falls_back_to_small() {
        let config: AppConfig = serde_json::from_str(
            r#"{
                "mouse_monitor": {
                    "required_shakes": 5,
                    "shake_time_limit": 1500,
                    "shake_threshold": 100
                },
                "autostart": false,
                "hotkey": "",
                "drop_size": "extra-large"
            }"#,
        )
        .unwrap();

        assert_eq!(config.drop_size, DropSize::Small);
    }
}

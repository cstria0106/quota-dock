use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const DEFAULT_SYNC_INTERVAL_SECS: u64 = 60;
pub const MIN_SYNC_INTERVAL_SECS: u64 = 30;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DesktopSettings {
    #[serde(default)]
    pub serial_port: String,
    #[serde(default)]
    pub wifi_ssid: String,
    #[serde(default)]
    pub device_url: String,
    #[serde(default = "default_sync_interval_secs")]
    pub sync_interval_secs: u64,
    #[serde(default = "default_brightness")]
    pub brightness: u8,
    #[serde(default)]
    pub disabled_provider_ids: BTreeSet<String>,
    #[serde(default)]
    pub provider_display: BTreeMap<String, ProviderDisplaySettings>,
    #[serde(default)]
    pub images: BTreeMap<String, PathBuf>,
    #[serde(default)]
    pub close_to_tray: bool,
    #[serde(default)]
    pub launch_at_startup: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProviderDisplaySettings {
    #[serde(default)]
    pub usage_windows: Vec<String>,
    #[serde(default = "default_show_provider_image")]
    pub show_image: bool,
    #[serde(default)]
    pub accent_color: Option<String>,
}

impl Default for ProviderDisplaySettings {
    fn default() -> Self {
        Self {
            usage_windows: Vec::new(),
            show_image: default_show_provider_image(),
            accent_color: None,
        }
    }
}

impl Default for DesktopSettings {
    fn default() -> Self {
        Self {
            serial_port: String::new(),
            wifi_ssid: String::new(),
            device_url: String::new(),
            sync_interval_secs: DEFAULT_SYNC_INTERVAL_SECS,
            brightness: default_brightness(),
            disabled_provider_ids: BTreeSet::new(),
            provider_display: BTreeMap::new(),
            images: BTreeMap::new(),
            close_to_tray: false,
            launch_at_startup: false,
        }
    }
}

impl DesktopSettings {
    pub fn normalized(mut self) -> Self {
        self.sync_interval_secs = self.sync_interval_secs.max(MIN_SYNC_INTERVAL_SECS);
        self.disabled_provider_ids = self
            .disabled_provider_ids
            .into_iter()
            .map(|provider_id| provider_id.to_ascii_lowercase())
            .collect();
        self.provider_display = self
            .provider_display
            .into_iter()
            .map(|(provider_id, settings)| {
                (
                    provider_id.to_ascii_lowercase(),
                    ProviderDisplaySettings {
                        usage_windows: dedupe_usage_windows(settings.usage_windows),
                        show_image: settings.show_image,
                        accent_color: settings.accent_color.filter(|color| is_hex_color(color)),
                    },
                )
            })
            .collect();
        self
    }
}

pub fn load_settings() -> (DesktopSettings, PathBuf, Option<String>) {
    let path = settings_path();
    match load_from_path(&path) {
        Ok(settings) => (settings, path, None),
        Err(err) if path.is_file() => (DesktopSettings::default(), path, Some(err)),
        Err(_) => (DesktopSettings::default(), path, None),
    }
}

pub fn load_from_path(path: &Path) -> Result<DesktopSettings, String> {
    let contents =
        fs::read_to_string(path).map_err(|err| format!("read {}: {err}", path.display()))?;
    toml::from_str::<DesktopSettings>(&contents)
        .map(DesktopSettings::normalized)
        .map_err(|err| format!("parse {}: {err}", path.display()))
}

pub fn save_to_path(path: &Path, settings: &DesktopSettings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("create {}: {err}", parent.display()))?;
    }
    let contents = toml::to_string_pretty(&settings.clone().normalized())
        .map_err(|err| format!("serialize settings: {err}"))?;
    fs::write(path, contents).map_err(|err| format!("write {}: {err}", path.display()))
}

fn settings_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("quota-dock")
        .join("desktop-settings.toml")
}

fn default_sync_interval_secs() -> u64 {
    DEFAULT_SYNC_INTERVAL_SECS
}

fn default_brightness() -> u8 {
    255
}

pub fn default_usage_window_limit() -> usize {
    3
}

fn default_show_provider_image() -> bool {
    true
}

fn dedupe_usage_windows(kinds: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    kinds
        .into_iter()
        .filter(|kind| !kind.trim().is_empty())
        .filter(|kind| seen.insert(kind.clone()))
        .take(default_usage_window_limit())
        .collect()
}

fn is_hex_color(value: &str) -> bool {
    let Some(hex) = value.trim().strip_prefix('#') else {
        return false;
    };
    hex.len() == 6 && hex.chars().all(|ch| ch.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_toml_has_no_wifi_password_field() {
        let settings = DesktopSettings {
            wifi_ssid: "studio".to_string(),
            ..Default::default()
        };

        let contents = toml::to_string(&settings).expect("serialize settings");

        assert!(contents.contains("wifi_ssid"));
        assert!(!contents.contains("password"));
    }

    #[test]
    fn normalizes_sync_interval_floor() {
        let settings = DesktopSettings {
            sync_interval_secs: 1,
            ..Default::default()
        }
        .normalized();

        assert_eq!(settings.sync_interval_secs, MIN_SYNC_INTERVAL_SECS);
    }
}

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const DEFAULT_APP_OFFSET: &str = "0x10000";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct MonitorConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub board: Option<BoardConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wifi: Option<WifiCredentials>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flash: Option<FlashConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageConfig>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct BoardConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WifiCredentials {
    pub ssid: String,
    pub password: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FlashConfig {
    pub firmware_bin: Option<PathBuf>,
    pub bootloader_bin: Option<PathBuf>,
    pub partition_table_bin: Option<PathBuf>,
    pub offset: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct UsageConfig {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub images: BTreeMap<String, PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlashInputs {
    pub firmware_bin: PathBuf,
    pub bootloader_bin: PathBuf,
    pub partition_table_bin: PathBuf,
    pub offset: String,
}

pub fn read_config_file(path: &Path) -> Result<MonitorConfig, String> {
    let contents =
        fs::read_to_string(path).map_err(|err| format!("read {}: {err}", path.display()))?;
    toml::from_str(&contents).map_err(|err| format!("parse {}: {err}", path.display()))
}

pub fn save_board_ip(path: &Path, ip: &str) -> Result<(), String> {
    let mut config = if path.is_file() {
        read_config_file(path)?
    } else {
        MonitorConfig::default()
    };
    config.board.get_or_insert_with(BoardConfig::default).ip = Some(ip.trim().to_string());
    write_config_file(path, &config)
}

pub fn resolve_device_url(
    config_path: &Path,
    device_url: Option<String>,
) -> Result<String, String> {
    if let Some(device_url) = device_url {
        return Ok(to_device_url(&device_url));
    }

    let config = read_config_file(config_path)?;
    let ip = config.board.and_then(|board| board.ip).ok_or_else(|| {
        format!(
            "missing board IP; pass a device URL or set [board].ip in {}",
            config_path.display()
        )
    })?;
    Ok(to_device_url(&ip))
}

pub fn resolve_flash_inputs(
    config_path: &Path,
    firmware_bin: Option<PathBuf>,
    bootloader_bin: Option<PathBuf>,
    partition_table_bin: Option<PathBuf>,
    offset: Option<String>,
) -> Result<FlashInputs, String> {
    let config = if config_path.is_file() {
        Some(read_config_file(config_path)?)
    } else {
        None
    };
    let flash_config = config.as_ref().and_then(|config| config.flash.as_ref());

    let firmware_bin = flash_path(
        "firmware bin",
        firmware_bin,
        flash_config.and_then(|flash| flash.firmware_bin.as_ref()),
        config_path,
    )?;
    let bootloader_bin = flash_path(
        "bootloader bin",
        bootloader_bin,
        flash_config.and_then(|flash| flash.bootloader_bin.as_ref()),
        config_path,
    )?;
    let partition_table_bin = flash_path(
        "partition table bin",
        partition_table_bin,
        flash_config.and_then(|flash| flash.partition_table_bin.as_ref()),
        config_path,
    )?;
    let offset = offset
        .or_else(|| flash_config.and_then(|flash| flash.offset.clone()))
        .unwrap_or_else(|| DEFAULT_APP_OFFSET.to_string());

    Ok(FlashInputs {
        firmware_bin,
        bootloader_bin,
        partition_table_bin,
        offset,
    })
}

fn flash_path(
    label: &str,
    cli_path: Option<PathBuf>,
    config_path_value: Option<&PathBuf>,
    config_file: &Path,
) -> Result<PathBuf, String> {
    if let Some(path) = cli_path {
        return Ok(path);
    }
    let path = config_path_value.ok_or_else(|| {
        format!(
            "missing {label}; pass it on the command line or set [flash] in {}",
            config_file.display()
        )
    })?;
    if path.is_absolute() {
        Ok(path.clone())
    } else {
        Ok(config_file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(path))
    }
}

fn write_config_file(path: &Path, config: &MonitorConfig) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|err| format!("create {}: {err}", parent.display()))?;
    }
    let contents = toml::to_string_pretty(config).map_err(|err| err.to_string())?;
    fs::write(path, contents).map_err(|err| format!("write {}: {err}", path.display()))
}

fn to_device_url(value: &str) -> String {
    let value = value.trim();
    if value.contains("://") {
        value.to_string()
    } else {
        format!("http://{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn cli_paths_override_config_paths() {
        let inputs = resolve_flash_inputs(
            Path::new("missing.toml"),
            Some(PathBuf::from("app.bin")),
            Some(PathBuf::from("boot.bin")),
            Some(PathBuf::from("part.bin")),
            None,
        )
        .expect("explicit paths should be enough");

        assert_eq!(inputs.firmware_bin, PathBuf::from("app.bin"));
        assert_eq!(inputs.bootloader_bin, PathBuf::from("boot.bin"));
        assert_eq!(inputs.partition_table_bin, PathBuf::from("part.bin"));
        assert_eq!(inputs.offset, DEFAULT_APP_OFFSET);
    }

    #[test]
    fn resolves_device_url_from_config_ip() {
        let dir = unique_temp_dir();
        fs::create_dir_all(&dir).expect("create temp dir");
        let config_path = dir.join("config.toml");
        fs::write(&config_path, "[board]\nip = \"192.168.4.20\"\n").expect("write config");

        let url = resolve_device_url(&config_path, None).expect("resolve device URL");

        assert_eq!(url, "http://192.168.4.20");
        fs::remove_file(&config_path).expect("remove config");
        fs::remove_dir(&dir).expect("remove temp dir");
    }

    #[test]
    fn explicit_device_url_overrides_config() {
        let url = resolve_device_url(
            Path::new("missing.toml"),
            Some("http://monitor.local".to_string()),
        )
        .expect("explicit URL should resolve");

        assert_eq!(url, "http://monitor.local");
    }

    #[test]
    fn saves_board_ip_without_dropping_existing_sections() {
        let dir = unique_temp_dir();
        fs::create_dir_all(&dir).expect("create temp dir");
        let config_path = dir.join("config.toml");
        fs::write(
            &config_path,
            "[wifi]\nssid = \"ssid\"\npassword = \"password\"\n",
        )
        .expect("write config");

        save_board_ip(&config_path, " 10.0.0.25 ").expect("save board IP");
        let config = read_config_file(&config_path).expect("read config");

        assert_eq!(
            config.board.and_then(|board| board.ip),
            Some("10.0.0.25".to_string())
        );
        assert_eq!(
            config.wifi.map(|wifi| (wifi.ssid, wifi.password)),
            Some(("ssid".to_string(), "password".to_string()))
        );
        fs::remove_file(&config_path).expect("remove config");
        fs::remove_dir(&dir).expect("remove temp dir");
    }

    fn unique_temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("monitor-config-test-{nanos}"))
    }
}

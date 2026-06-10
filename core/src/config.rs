use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const DEFAULT_APP_OFFSET: &str = "0x10000";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MonitorConfig {
    pub wifi: Option<WifiCredentials>,
    pub flash: Option<FlashConfig>,
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

#[cfg(test)]
mod tests {
    use super::*;

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
}

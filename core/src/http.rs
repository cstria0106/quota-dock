use std::time::Duration;

use serde::Serialize;

use crate::usage::UsageSnapshot;
use crate::{ApiResponse, DeviceCommand, StatusResponse};

pub const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 4;

pub fn http_status(device_url: &str) -> Result<StatusResponse, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(DEFAULT_HTTP_TIMEOUT_SECS))
        .build()
        .map_err(|err| err.to_string())?
        .get(url(device_url, "/status"))
        .send()
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json::<StatusResponse>()
        .map_err(|err| err.to_string())
}

pub fn http_usage(device_url: &str, snapshot: &UsageSnapshot) -> Result<ApiResponse, String> {
    http_post_json(device_url, "/usage", snapshot)
}

pub fn http_command(device_url: &str, command: &DeviceCommand) -> Result<ApiResponse, String> {
    http_post_json(device_url, "/command", command)
}

fn http_post_json<T: Serialize>(
    device_url: &str,
    path: &str,
    body: &T,
) -> Result<ApiResponse, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(DEFAULT_HTTP_TIMEOUT_SECS))
        .build()
        .map_err(|err| err.to_string())?
        .post(url(device_url, path))
        .json(body)
        .send()
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json::<ApiResponse>()
        .map_err(|err| err.to_string())
}

pub fn url(device_url: &str, path: &str) -> String {
    format!("{}{}", device_url.trim_end_matches('/'), path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_trailing_slashes_from_device_url() {
        assert_eq!(url("http://device/", "/status"), "http://device/status");
    }
}

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::usage::{UsageProviderUpdate, UsageSnapshot};
use crate::{ApiResponse, DeviceCommand, StatusResponse};

pub const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 4;
const DEVICE_PROTOCOL_PORT: u16 = 3333;
const MAX_FRAME_BODY: usize = 64 * 1024;

pub fn http_status(device_url: &str) -> Result<StatusResponse, String> {
    match send_request(device_url, &DeviceRequest::Status)? {
        DeviceReply::Status(status) => Ok(status),
        DeviceReply::Api(response) => Err(format!("expected status reply: {}", response.message)),
    }
}

pub fn http_usage(device_url: &str, snapshot: &UsageSnapshot) -> Result<ApiResponse, String> {
    match send_request(device_url, &DeviceRequest::Usage(snapshot))? {
        DeviceReply::Api(response) => Ok(response),
        DeviceReply::Status(_) => Err("expected api reply, got status".to_string()),
    }
}

pub fn http_usage_provider(
    device_url: &str,
    update: &UsageProviderUpdate,
) -> Result<ApiResponse, String> {
    match send_request(device_url, &DeviceRequest::UsageProvider(update))? {
        DeviceReply::Api(response) => Ok(response),
        DeviceReply::Status(_) => Err("expected api reply, got status".to_string()),
    }
}

pub fn http_command(device_url: &str, command: &DeviceCommand) -> Result<ApiResponse, String> {
    match send_request(device_url, &DeviceRequest::Command(command))? {
        DeviceReply::Api(response) => Ok(response),
        DeviceReply::Status(_) => Err("expected api reply, got status".to_string()),
    }
}

fn send_request(device_url: &str, request: &DeviceRequest<'_>) -> Result<DeviceReply, String> {
    let body = postcard::to_allocvec(request).map_err(|err| err.to_string())?;
    let mut stream = connect(device_url)?;
    write_frame(&mut stream, &body)?;
    let reply = read_frame(&mut stream)?;
    postcard::from_bytes(&reply).map_err(|err| err.to_string())
}

pub fn postcard_len<T: Serialize>(body: &T) -> Result<usize, String> {
    postcard::to_allocvec(body)
        .map(|body| body.len())
        .map_err(|err| err.to_string())
}

fn connect(device_url: &str) -> Result<TcpStream, String> {
    let address = device_address(device_url);
    let timeout = Duration::from_secs(DEFAULT_HTTP_TIMEOUT_SECS);
    let socket_address = address
        .to_socket_addrs()
        .map_err(|err| format!("resolve {address}: {err}"))?
        .next()
        .ok_or_else(|| format!("resolve {address}: no address"))?;
    let stream = TcpStream::connect_timeout(&socket_address, timeout)
        .map_err(|err| format!("connect {address}: {err}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|err| format!("set read timeout: {err}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|err| format!("set write timeout: {err}"))?;
    Ok(stream)
}

fn write_frame(stream: &mut TcpStream, body: &[u8]) -> Result<(), String> {
    if body.len() > MAX_FRAME_BODY {
        return Err(format!("frame too large: {} bytes", body.len()));
    }
    stream
        .write_all(&(body.len() as u32).to_le_bytes())
        .and_then(|_| stream.write_all(body))
        .and_then(|_| stream.flush())
        .map_err(|err| format!("write frame: {err}"))
}

fn read_frame(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut len_bytes = [0_u8; 4];
    stream
        .read_exact(&mut len_bytes)
        .map_err(|err| format!("read frame length: {err}"))?;
    let len = u32::from_le_bytes(len_bytes) as usize;
    if len > MAX_FRAME_BODY {
        return Err(format!("frame too large: {len} bytes"));
    }

    let mut body = vec![0; len];
    stream
        .read_exact(&mut body)
        .map_err(|err| format!("read frame body: {err}"))?;
    Ok(body)
}

fn device_address(device_url: &str) -> String {
    let value = device_url.trim();
    let value = value
        .strip_prefix("http://")
        .or_else(|| value.strip_prefix("https://"))
        .unwrap_or(value);
    let host = value
        .split('/')
        .next()
        .unwrap_or(value)
        .trim_end_matches('/');
    if host.contains(':') {
        host.to_string()
    } else {
        format!("{host}:{DEVICE_PROTOCOL_PORT}")
    }
}

pub fn url(device_url: &str, path: &str) -> String {
    format!("{}{}", device_url.trim_end_matches('/'), path)
}

#[derive(Serialize)]
enum DeviceRequest<'a> {
    Status,
    Command(&'a DeviceCommand),
    Usage(&'a UsageSnapshot),
    UsageProvider(&'a UsageProviderUpdate),
}

#[derive(Deserialize)]
enum DeviceReply {
    Status(StatusResponse),
    Api(ApiResponse),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_trailing_slashes_from_device_url() {
        assert_eq!(url("http://device/", "/status"), "http://device/status");
    }

    #[test]
    fn builds_device_protocol_address() {
        assert_eq!(device_address("http://192.168.1.50"), "192.168.1.50:3333");
        assert_eq!(device_address("192.168.1.50:4444"), "192.168.1.50:4444");
    }
}

use std::io::{Read, Write};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serialport::ClearBuffer;

use crate::{ApiResponse, DeviceCommand, StatusResponse};

const SERIAL_FRAME_MAGIC: &[u8; 4] = b"MONP";
const SERIAL_MAX_FRAME_BODY: usize = 64 * 1024;
const SERIAL_OPEN_DELAY_MS: u64 = 500;
const SERIAL_RETRY_INTERVAL_MS: u64 = 500;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SerialRequest {
    Status,
    SetWifi { ssid: String, password: String },
    ClearWifi,
    Command(DeviceCommand),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SerialReply {
    Status(StatusResponse),
    Api(ApiResponse),
}

pub fn serial_port_names() -> Result<Vec<String>, String> {
    serialport::available_ports()
        .map_err(|err| err.to_string())
        .map(|ports| ports.into_iter().map(|port| port.port_name).collect())
}

pub fn send_serial(
    port_name: &str,
    baud: u32,
    request: &SerialRequest,
    timeout: Duration,
) -> Result<ApiResponse, String> {
    let reply = send_serial_reply(port_name, baud, request, timeout)?;
    match reply {
        SerialReply::Api(response) => Ok(response),
        SerialReply::Status(_) => Err("expected api serial reply, got status".to_string()),
    }
}

pub fn send_serial_status(
    port_name: &str,
    baud: u32,
    timeout: Duration,
) -> Result<StatusResponse, String> {
    let reply = send_serial_reply(port_name, baud, &SerialRequest::Status, timeout)?;
    match reply {
        SerialReply::Status(status) => Ok(status),
        SerialReply::Api(response) => Err(format!(
            "expected status serial reply: {}",
            response.message
        )),
    }
}

fn send_serial_reply(
    port_name: &str,
    baud: u32,
    request: &SerialRequest,
    timeout: Duration,
) -> Result<SerialReply, String> {
    match send_serial_once(port_name, baud, request, timeout, true) {
        Ok(response) => Ok(response),
        Err(err) if is_retryable_serial_timeout(&err) => {
            send_serial_once(port_name, baud, request, timeout, false)
        }
        Err(err) => Err(err),
    }
}

fn is_retryable_serial_timeout(err: &str) -> bool {
    err == "serial response timed out" || err.to_ascii_lowercase().contains("timed out")
}

fn send_serial_once(
    port_name: &str,
    baud: u32,
    request: &SerialRequest,
    timeout: Duration,
    data_terminal_ready: bool,
) -> Result<SerialReply, String> {
    configure_serial_terminal(port_name, baud)?;
    let mut port = serialport::new(port_name, baud)
        .timeout(Duration::from_millis(100))
        .dtr_on_open(data_terminal_ready)
        .open()
        .map_err(|err| format!("open serial {port_name}: {err}"))?;
    port.write_data_terminal_ready(data_terminal_ready)
        .map_err(|err| format!("set serial DTR: {err}"))?;
    port.write_request_to_send(false)
        .map_err(|err| format!("set serial RTS: {err}"))?;
    thread::sleep(Duration::from_millis(SERIAL_OPEN_DELAY_MS));
    let _ = port.clear(ClearBuffer::Input);
    let request = postcard::to_allocvec(request).map_err(|err| err.to_string())?;

    let started = Instant::now();
    let mut last_write = None;
    let mut input = Vec::new();
    let mut buffer = [0_u8; 64];
    while started.elapsed() < timeout {
        if last_write
            .map(|last_write: Instant| {
                last_write.elapsed() >= Duration::from_millis(SERIAL_RETRY_INTERVAL_MS)
            })
            .unwrap_or(true)
        {
            match write_serial_frame(&mut port, &request) {
                Ok(()) => {
                    last_write = Some(Instant::now());
                }
                Err(err) if is_retryable_serial_timeout(&err) => {
                    thread::sleep(Duration::from_millis(50));
                }
                Err(err) => return Err(err),
            }
        }

        match port.read(&mut buffer) {
            Ok(0) => thread::sleep(Duration::from_millis(50)),
            Ok(len) => {
                input.extend_from_slice(&buffer[..len]);
                if let Some(reply) = try_read_serial_reply(&mut input)? {
                    return Ok(reply);
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::TimedOut => {}
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(err) => return Err(format!("serial read failed: {err}")),
        }
    }

    Err("serial response timed out".to_string())
}

fn write_serial_frame(port: &mut dyn Write, body: &[u8]) -> Result<(), String> {
    if body.len() > SERIAL_MAX_FRAME_BODY {
        return Err(format!("serial frame too large: {} bytes", body.len()));
    }
    port.write_all(SERIAL_FRAME_MAGIC)
        .and_then(|_| port.write_all(&(body.len() as u32).to_le_bytes()))
        .and_then(|_| port.write_all(body))
        .map_err(|err| err.to_string())
}

fn try_read_serial_reply(input: &mut Vec<u8>) -> Result<Option<SerialReply>, String> {
    let Some(start) = input
        .windows(SERIAL_FRAME_MAGIC.len())
        .position(|window| window == SERIAL_FRAME_MAGIC)
    else {
        let keep = SERIAL_FRAME_MAGIC.len().saturating_sub(1);
        if input.len() > keep {
            input.drain(..input.len() - keep);
        }
        return Ok(None);
    };
    if start > 0 {
        input.drain(..start);
    }
    if input.len() < SERIAL_FRAME_MAGIC.len() + 4 {
        return Ok(None);
    }

    let len_offset = SERIAL_FRAME_MAGIC.len();
    let len = u32::from_le_bytes(
        input[len_offset..len_offset + 4]
            .try_into()
            .map_err(|_| "invalid serial frame length".to_string())?,
    ) as usize;
    if len > SERIAL_MAX_FRAME_BODY {
        return Err(format!("serial frame too large: {len} bytes"));
    }

    let body_offset = len_offset + 4;
    let frame_len = body_offset + len;
    if input.len() < frame_len {
        return Ok(None);
    }

    let body = input[body_offset..frame_len].to_vec();
    input.drain(..frame_len);
    postcard::from_bytes(&body)
        .map(Some)
        .map_err(|err| err.to_string())
}

#[cfg(unix)]
fn configure_serial_terminal(port_name: &str, baud: u32) -> Result<(), String> {
    let status = Command::new("stty")
        .arg(stty_device_flag())
        .arg(port_name)
        .arg(baud.to_string())
        .arg("raw")
        .arg("-echo")
        .status()
        .map_err(|err| format!("failed to run stty: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("stty exited with {status}"))
    }
}

#[cfg(all(unix, target_os = "macos"))]
fn stty_device_flag() -> &'static str {
    "-f"
}

#[cfg(all(unix, not(target_os = "macos")))]
fn stty_device_flag() -> &'static str {
    "-F"
}

#[cfg(not(unix))]
fn configure_serial_terminal(_: &str, _: u32) -> Result<(), String> {
    Ok(())
}

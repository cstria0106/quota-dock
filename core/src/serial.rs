use std::io::{Read, Write};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serialport::ClearBuffer;

use crate::{ApiResponse, DeviceCommand};

const SERIAL_OPEN_DELAY_MS: u64 = 500;
const SERIAL_RETRY_INTERVAL_MS: u64 = 500;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SerialRequest {
    Status,
    SetWifi { ssid: String, password: String },
    ClearWifi,
    Command { command: DeviceCommand },
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
    match send_serial_once(port_name, baud, request, timeout, true) {
        Ok(response) => Ok(response),
        Err(err) if err == "serial response timed out" => {
            send_serial_once(port_name, baud, request, timeout, false)
        }
        Err(err) => Err(err),
    }
}

fn send_serial_once(
    port_name: &str,
    baud: u32,
    request: &SerialRequest,
    timeout: Duration,
    data_terminal_ready: bool,
) -> Result<ApiResponse, String> {
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
    let request = serde_json::to_string(request).map_err(|err| err.to_string())?;

    let started = Instant::now();
    let mut last_write = None;
    let mut line = Vec::new();
    let mut buffer = [0_u8; 64];
    while started.elapsed() < timeout {
        if last_write
            .map(|last_write: Instant| {
                last_write.elapsed() >= Duration::from_millis(SERIAL_RETRY_INTERVAL_MS)
            })
            .unwrap_or(true)
        {
            writeln!(port, "{request}").map_err(|err| err.to_string())?;
            port.flush().map_err(|err| err.to_string())?;
            last_write = Some(Instant::now());
        }

        match port.read(&mut buffer) {
            Ok(0) => thread::sleep(Duration::from_millis(50)),
            Ok(len) => {
                for byte in &buffer[..len] {
                    if *byte == b'\n' {
                        if let Ok(text) = std::str::from_utf8(&line) {
                            let text = text.trim();
                            if text.starts_with('{')
                                && let Ok(response) = serde_json::from_str::<ApiResponse>(text)
                            {
                                return Ok(response);
                            }
                        }
                        line.clear();
                    } else if line.len() < 4096 {
                        line.push(*byte);
                    } else {
                        line.clear();
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::TimedOut => {}
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(err) => return Err(format!("serial read failed: {err}")),
        }
    }

    Err("serial response timed out".to_string())
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

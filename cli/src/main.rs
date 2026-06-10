mod usage;

use std::borrow::Cow;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand, ValueEnum};
use espflash::connection::{Connection, ResetAfterOperation, ResetBeforeOperation};
use espflash::flasher::Flasher;
use espflash::image_format::Segment;
use espflash::target::{Chip, DefaultProgressCallback};
use serde::{Deserialize, Serialize};
use serialport::{ClearBuffer, FlowControl, SerialPortType, UsbPortInfo};
use usage::{ProviderSelection, UsageSnapshot};

#[cfg(windows)]
const DEFAULT_PORT: &str = "COM3";
#[cfg(not(windows))]
const DEFAULT_PORT: &str = "/dev/ttyACM0";
const DEFAULT_BAUD: u32 = 115_200;
const DEFAULT_CONFIG_FILE: &str = "config.toml";
const DEFAULT_APP_OFFSET: &str = "0x10000";
const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 4;
const SERIAL_OPEN_DELAY_MS: u64 = 500;
const SERIAL_RETRY_INTERVAL_MS: u64 = 500;

#[derive(Parser)]
#[command(
    author,
    version,
    about = "Provision and control the monitor ESP32-S3 firmware"
)]
struct Cli {
    #[arg(long, default_value = DEFAULT_PORT, global = true)]
    port: String,

    #[arg(long, default_value_t = DEFAULT_BAUD, global = true)]
    baud: u32,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Ports,
    Flash {
        firmware_bin: Option<PathBuf>,
        #[arg(long, default_value = DEFAULT_CONFIG_FILE)]
        config: PathBuf,
        #[arg(long)]
        bootloader_bin: Option<PathBuf>,
        #[arg(long)]
        partition_table_bin: Option<PathBuf>,
        #[arg(long)]
        offset: Option<String>,
    },
    Reset,
    ClearWifi,
    Provision {
        #[arg(long, default_value = DEFAULT_CONFIG_FILE)]
        config: PathBuf,
    },
    SerialStatus,
    HttpStatus {
        device_url: String,
    },
    Usage {
        #[arg(value_enum, default_value_t = UsageProviderArg::All)]
        provider: UsageProviderArg,
        #[arg(long)]
        json: bool,
    },
    PushUsage {
        device_url: String,
        #[arg(value_enum, default_value_t = UsageProviderArg::All)]
        provider: UsageProviderArg,
    },
    WatchUsage {
        device_url: String,
        #[arg(value_enum, default_value_t = UsageProviderArg::All)]
        provider: UsageProviderArg,
        #[arg(long, default_value_t = 60)]
        interval_secs: u64,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum UsageProviderArg {
    All,
    Codex,
    Claude,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SerialRequest {
    Status,
    SetWifi { ssid: String, password: String },
    ClearWifi,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    ok: bool,
    message: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct StatusResponse {
    connected: bool,
    ip: Option<String>,
}

fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Commands::Ports => list_ports(),
        Commands::Flash {
            firmware_bin,
            config,
            bootloader_bin,
            partition_table_bin,
            offset,
        } => {
            let inputs = resolve_flash_inputs(
                &config,
                firmware_bin,
                bootloader_bin,
                partition_table_bin,
                offset,
            )?;
            flash_bin(
                &inputs.firmware_bin,
                &inputs.bootloader_bin,
                &inputs.partition_table_bin,
                &inputs.offset,
                &cli.port,
                cli.baud,
            )
        }
        Commands::Reset => reset_device(&cli.port, cli.baud),
        Commands::ClearWifi => {
            let response = send_serial(
                &cli.port,
                cli.baud,
                &SerialRequest::ClearWifi,
                Duration::from_secs(30),
            )?;
            print_api_response("clear wifi", &response);
            Ok(())
        }
        Commands::Provision { config } => {
            let credentials = read_config_file(&config)?
                .wifi
                .ok_or_else(|| format!("{} has no [wifi] section", config.display()))?;
            if credentials.ssid.trim().is_empty() {
                return Err(format!("{} has an empty Wi-Fi SSID", config.display()));
            }
            let response = send_serial(
                &cli.port,
                cli.baud,
                &SerialRequest::SetWifi {
                    ssid: credentials.ssid,
                    password: credentials.password,
                },
                Duration::from_secs(30),
            )?;
            print_api_response("provision", &response);
            Ok(())
        }
        Commands::SerialStatus => {
            let response = send_serial(
                &cli.port,
                cli.baud,
                &SerialRequest::Status,
                Duration::from_secs(6),
            )?;
            print_api_response("serial status", &response);
            Ok(())
        }
        Commands::HttpStatus { device_url } => {
            let status = http_status(&device_url)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&status).map_err(|err| err.to_string())?
            );
            Ok(())
        }
        Commands::Usage { provider, json } => {
            let snapshot = usage::collect_snapshot(to_provider_selection(provider));
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&snapshot).map_err(|err| err.to_string())?
                );
            } else {
                print_usage_snapshot(&snapshot);
            }
            Ok(())
        }
        Commands::PushUsage {
            device_url,
            provider,
        } => {
            let snapshot = usage::collect_snapshot(to_provider_selection(provider));
            let response = http_usage(&device_url, &snapshot)?;
            print_api_response("push usage", &response);
            Ok(())
        }
        Commands::WatchUsage {
            device_url,
            provider,
            interval_secs,
        } => watch_usage(
            &device_url,
            to_provider_selection(provider),
            Duration::from_secs(interval_secs.max(5)),
        ),
    }
}

fn list_ports() -> Result<(), String> {
    let ports = serialport::available_ports().map_err(|err| err.to_string())?;
    for port in ports {
        println!("{}", port.port_name);
    }
    Ok(())
}

fn resolve_flash_inputs(
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

fn flash_bin(
    firmware_bin: &Path,
    bootloader_bin: &Path,
    partition_table_bin: &Path,
    offset: &str,
    port: &str,
    baud: u32,
) -> Result<(), String> {
    if !firmware_bin.is_file() {
        return Err(format!(
            "firmware bin does not exist: {}",
            firmware_bin.display()
        ));
    }
    if !bootloader_bin.is_file() {
        return Err(format!(
            "bootloader bin does not exist: {}",
            bootloader_bin.display()
        ));
    }
    if !partition_table_bin.is_file() {
        return Err(format!(
            "partition table bin does not exist: {}",
            partition_table_bin.display()
        ));
    }

    let app_offset = parse_u32(offset)?;
    let firmware = fs::read(firmware_bin)
        .map_err(|err| format!("read firmware bin {}: {err}", firmware_bin.display()))?;
    let bootloader = fs::read(bootloader_bin)
        .map_err(|err| format!("read bootloader bin {}: {err}", bootloader_bin.display()))?;
    let partition_table = fs::read(partition_table_bin).map_err(|err| {
        format!(
            "read partition table bin {}: {err}",
            partition_table_bin.display()
        )
    })?;

    let segments = [
        Segment {
            addr: app_offset,
            data: Cow::Borrowed(firmware.as_slice()),
        },
        Segment {
            addr: 0x0,
            data: Cow::Borrowed(bootloader.as_slice()),
        },
        Segment {
            addr: 0x8000,
            data: Cow::Borrowed(partition_table.as_slice()),
        },
    ];
    let mut flasher = connect_flasher(port, baud)?;
    let mut progress = DefaultProgressCallback;
    flasher
        .write_bins_to_flash(&segments, &mut progress)
        .map_err(|err| format!("flash failed: {err}"))
}

fn parse_u32(value: &str) -> Result<u32, String> {
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16).map_err(|err| format!("invalid offset {value}: {err}"))
    } else {
        value
            .parse::<u32>()
            .map_err(|err| format!("invalid offset {value}: {err}"))
    }
}

fn connect_flasher(port: &str, baud: u32) -> Result<Flasher, String> {
    let port_info = serialport::available_ports()
        .map_err(|err| err.to_string())?
        .into_iter()
        .find(|info| info.port_name == port);
    let usb_info = match port_info.map(|info| info.port_type) {
        Some(SerialPortType::UsbPort(info)) => info,
        _ => UsbPortInfo {
            vid: 0,
            pid: 0,
            serial_number: None,
            manufacturer: None,
            product: None,
        },
    };
    let serial = serialport::new(port, 115_200)
        .flow_control(FlowControl::None)
        .open_native()
        .map_err(|err| format!("open serial {port}: {err}"))?;
    let connection = Connection::new(
        serial,
        usb_info,
        ResetAfterOperation::HardReset,
        ResetBeforeOperation::DefaultReset,
        baud,
    );
    Flasher::connect(
        connection,
        true,
        true,
        true,
        Some(Chip::Esp32s3),
        Some(baud),
    )
    .map_err(|err| format!("connect flasher: {err}"))
}

fn reset_device(port: &str, baud: u32) -> Result<(), String> {
    let mut flasher = connect_flasher(port, baud)?;
    flasher
        .connection()
        .reset()
        .map_err(|err| format!("reset device: {err}"))
}

fn send_serial(
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
                            if text.starts_with('{') {
                                if let Ok(response) = serde_json::from_str::<ApiResponse>(text) {
                                    return Ok(response);
                                }
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

fn http_status(device_url: &str) -> Result<StatusResponse, String> {
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

fn http_usage(device_url: &str, snapshot: &UsageSnapshot) -> Result<ApiResponse, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(DEFAULT_HTTP_TIMEOUT_SECS))
        .build()
        .map_err(|err| err.to_string())?
        .post(url(device_url, "/usage"))
        .json(snapshot)
        .send()
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json::<ApiResponse>()
        .map_err(|err| err.to_string())
}

fn url(device_url: &str, path: &str) -> String {
    format!("{}{}", device_url.trim_end_matches('/'), path)
}

fn read_config_file(path: &Path) -> Result<MonitorConfig, String> {
    let contents =
        fs::read_to_string(path).map_err(|err| format!("read {}: {err}", path.display()))?;
    let config: MonitorConfig =
        toml::from_str(&contents).map_err(|err| format!("parse {}: {err}", path.display()))?;
    Ok(config)
}

fn print_api_response(label: &str, response: &ApiResponse) {
    println!("{label}: ok={} message={}", response.ok, response.message);
}

fn print_usage_snapshot(snapshot: &UsageSnapshot) {
    println!("updated: {}", snapshot.updated_at);
    for provider in &snapshot.providers {
        let plan = provider.plan.as_deref().unwrap_or("-");
        println!(
            "{} source={} plan={}",
            provider.label, provider.source, plan
        );
        for window in &provider.windows {
            let reset = window.resets_at.as_deref().unwrap_or("-");
            println!(
                "  {:<8} {:>3}% {:<9} reset={}",
                window.label, window.used_percent, window.status, reset
            );
        }
    }
}

fn watch_usage(
    device_url: &str,
    provider: ProviderSelection,
    interval: Duration,
) -> Result<(), String> {
    loop {
        let snapshot = usage::collect_snapshot(provider);
        let response = http_usage(device_url, &snapshot)?;
        print_api_response("push usage", &response);
        thread::sleep(interval);
    }
}

fn to_provider_selection(provider: UsageProviderArg) -> ProviderSelection {
    match provider {
        UsageProviderArg::All => ProviderSelection::All,
        UsageProviderArg::Codex => ProviderSelection::Codex,
        UsageProviderArg::Claude => ProviderSelection::Claude,
    }
}

#[derive(Debug, Deserialize)]
struct MonitorConfig {
    wifi: Option<WifiCredentials>,
    flash: Option<FlashConfig>,
}

#[derive(Debug, Deserialize)]
struct WifiCredentials {
    ssid: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct FlashConfig {
    firmware_bin: Option<PathBuf>,
    bootloader_bin: Option<PathBuf>,
    partition_table_bin: Option<PathBuf>,
    offset: Option<String>,
}

struct FlashInputs {
    firmware_bin: PathBuf,
    bootloader_bin: PathBuf,
    partition_table_bin: PathBuf,
    offset: String,
}

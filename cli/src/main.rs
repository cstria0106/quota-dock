mod usage;

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use serialport::ClearBuffer;
use usage::{ProviderSelection, UsageSnapshot};

#[cfg(windows)]
const DEFAULT_PORT: &str = "COM3";
#[cfg(not(windows))]
const DEFAULT_PORT: &str = "/dev/ttyACM0";
const DEFAULT_BAUD: u32 = 115_200;
const DEFAULT_CONFIG_FILE: &str = "monitor.config.json";
const DEFAULT_FIRMWARE_DIR: &str = "../firmware";
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
        #[arg(long, default_value = DEFAULT_FIRMWARE_DIR)]
        firmware_dir: PathBuf,
        #[arg(long)]
        skip_build: bool,
    },
    Reset {
        #[arg(long, default_value = DEFAULT_FIRMWARE_DIR)]
        firmware_dir: PathBuf,
    },
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
            firmware_dir,
            skip_build,
        } => {
            if !skip_build {
                build(&firmware_dir)?;
            }
            flash(&firmware_dir, &cli.port)
        }
        Commands::Reset { firmware_dir } => reset_device(&firmware_dir, &cli.port),
        Commands::Provision { config } => {
            let credentials = read_config_file(&config)?.wifi;
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

fn build(firmware_dir: &Path) -> Result<(), String> {
    let status = Command::new("./scripts/build.sh")
        .current_dir(firmware_dir)
        .status()
        .map_err(|err| format!("failed to run build script: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("build script exited with {status}"))
    }
}

fn flash(firmware_dir: &Path, port: &str) -> Result<(), String> {
    let espflash = espflash_path(firmware_dir);
    let partition_csv = firmware_dir.join("partitions.csv");
    let partition_bin = firmware_dir.join("target").join("partition-table.bin");
    let target_dir = firmware_dir
        .join("target")
        .join("xtensa-esp32s3-espidf")
        .join("release");
    let app = target_dir.join("agent-quota-monitor");
    let bootloader = target_dir.join("bootloader.bin");

    run_command(
        Command::new(&espflash)
            .arg("partition-table")
            .arg("--to-binary")
            .arg("--output")
            .arg(&partition_bin)
            .arg(&partition_csv),
        "espflash partition-table",
    )?;
    run_command(
        Command::new(&espflash)
            .arg("flash")
            .arg("--port")
            .arg(port)
            .arg(&app),
        "espflash flash",
    )?;
    run_command(
        Command::new(&espflash)
            .arg("write-bin")
            .arg("--port")
            .arg(port)
            .arg("0x0")
            .arg(&bootloader),
        "espflash write bootloader",
    )?;
    run_command(
        Command::new(&espflash)
            .arg("write-bin")
            .arg("--port")
            .arg(port)
            .arg("0x8000")
            .arg(&partition_bin),
        "espflash write partition table",
    )
}

fn reset_device(firmware_dir: &Path, port: &str) -> Result<(), String> {
    let espflash = espflash_path(firmware_dir);
    run_command(
        Command::new(&espflash).arg("reset").arg("--port").arg(port),
        "espflash reset",
    )
}

fn espflash_path(firmware_dir: &Path) -> PathBuf {
    let binary = if cfg!(windows) {
        "espflash.exe"
    } else {
        "espflash"
    };
    let local = firmware_dir.join(".tools").join("bin").join(binary);
    if local.exists() {
        local
    } else {
        PathBuf::from(binary)
    }
}

fn run_command(command: &mut Command, label: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|err| format!("failed to run {label}: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{label} exited with {status}"))
    }
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
    let config: MonitorConfig = serde_json::from_str(&contents)
        .map_err(|err| format!("parse {}: {err}", path.display()))?;
    if config.wifi.ssid.trim().is_empty() {
        return Err(format!("{} has an empty Wi-Fi SSID", path.display()));
    }
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
    wifi: WifiCredentials,
}

#[derive(Debug, Deserialize)]
struct WifiCredentials {
    ssid: String,
    password: String,
}

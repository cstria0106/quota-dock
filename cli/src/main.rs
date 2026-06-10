use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use monitor_core::config::{
    read_config_file, resolve_device_url, resolve_flash_inputs, save_board_ip,
};
use monitor_core::flash::{flash_firmware, reset_device};
use monitor_core::http::{http_status, http_usage};
use monitor_core::serial::{send_serial, serial_port_names};
use monitor_core::usage::collect_snapshot;
use monitor_core::{ApiResponse, ProviderSelection, SerialRequest, StatusResponse, UsageSnapshot};

#[cfg(windows)]
const DEFAULT_PORT: &str = "COM3";
#[cfg(not(windows))]
const DEFAULT_PORT: &str = "/dev/ttyACM0";
const DEFAULT_BAUD: u32 = 115_200;
const DEFAULT_CONFIG_FILE: &str = "config.toml";

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

    #[arg(long, default_value = DEFAULT_CONFIG_FILE, global = true)]
    config: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Ports,
    Flash {
        firmware_bin: Option<PathBuf>,
        #[arg(long)]
        bootloader_bin: Option<PathBuf>,
        #[arg(long)]
        partition_table_bin: Option<PathBuf>,
        #[arg(long)]
        offset: Option<String>,
    },
    Reset,
    ClearWifi,
    Provision,
    SerialStatus,
    HttpStatus {
        device_url: Option<String>,
    },
    Usage {
        #[arg(value_enum, default_value_t = UsageProviderArg::All)]
        provider: UsageProviderArg,
        #[arg(long)]
        json: bool,
    },
    PushUsage {
        #[arg(value_name = "DEVICE_URL_OR_PROVIDER")]
        device_url_or_provider: Option<String>,
        #[arg(value_enum)]
        provider: Option<UsageProviderArg>,
    },
    WatchUsage {
        #[arg(value_name = "DEVICE_URL_OR_PROVIDER")]
        device_url_or_provider: Option<String>,
        #[arg(value_enum)]
        provider: Option<UsageProviderArg>,
        #[arg(long, default_value_t = 60)]
        interval_secs: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum UsageProviderArg {
    All,
    Codex,
    Claude,
}

fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let Cli {
        port,
        baud,
        config,
        command,
    } = cli;
    match command {
        Commands::Ports => list_ports(),
        Commands::Flash {
            firmware_bin,
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
            flash_firmware(&inputs, &port, baud)
        }
        Commands::Reset => reset_device(&port, baud),
        Commands::ClearWifi => {
            let response = send_serial(
                &port,
                baud,
                &SerialRequest::ClearWifi,
                Duration::from_secs(30),
            )?;
            print_api_response("clear wifi", &response);
            Ok(())
        }
        Commands::Provision => {
            let credentials = read_config_file(&config)?
                .wifi
                .ok_or_else(|| format!("{} has no [wifi] section", config.display()))?;
            if credentials.ssid.trim().is_empty() {
                return Err(format!("{} has an empty Wi-Fi SSID", config.display()));
            }
            let response = send_serial(
                &port,
                baud,
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
            let response =
                send_serial(&port, baud, &SerialRequest::Status, Duration::from_secs(6))?;
            print_api_response("serial status", &response);
            save_status_ip(&config, &response)?;
            Ok(())
        }
        Commands::HttpStatus { device_url } => {
            let device_url = resolve_device_url(&config, device_url)?;
            let status = http_status(&device_url)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&status).map_err(|err| err.to_string())?
            );
            Ok(())
        }
        Commands::Usage { provider, json } => {
            let snapshot = collect_snapshot(to_provider_selection(provider));
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
            device_url_or_provider,
            provider,
        } => {
            let (device_url, provider) =
                resolve_usage_target(&config, device_url_or_provider, provider)?;
            let snapshot = collect_snapshot(to_provider_selection(provider));
            let response = http_usage(&device_url, &snapshot)?;
            print_api_response("push usage", &response);
            Ok(())
        }
        Commands::WatchUsage {
            device_url_or_provider,
            provider,
            interval_secs,
        } => {
            let (device_url, provider) =
                resolve_usage_target(&config, device_url_or_provider, provider)?;
            watch_usage(
                &device_url,
                to_provider_selection(provider),
                Duration::from_secs(interval_secs.max(5)),
            )
        }
    }
}

fn list_ports() -> Result<(), String> {
    for port in serial_port_names()? {
        println!("{port}");
    }
    Ok(())
}

fn print_api_response(label: &str, response: &ApiResponse) {
    println!("{label}: ok={} message={}", response.ok, response.message);
}

fn save_status_ip(config: &Path, response: &ApiResponse) -> Result<(), String> {
    if !response.ok {
        return Ok(());
    }
    let status = serde_json::from_str::<StatusResponse>(&response.message)
        .map_err(|err| format!("parse serial status response: {err}"))?;
    let Some(ip) = status
        .ip
        .as_deref()
        .map(str::trim)
        .filter(|ip| !ip.is_empty())
    else {
        println!("serial status: no board IP to save");
        return Ok(());
    };

    save_board_ip(config, ip)?;
    println!("saved board IP {ip} to {}", config.display());
    Ok(())
}

fn resolve_usage_target(
    config: &Path,
    device_url_or_provider: Option<String>,
    provider: Option<UsageProviderArg>,
) -> Result<(String, UsageProviderArg), String> {
    if provider.is_none()
        && let Some(value) = device_url_or_provider.as_deref()
        && let Some(provider) = parse_provider_arg(value)
    {
        return Ok((resolve_device_url(config, None)?, provider));
    }

    Ok((
        resolve_device_url(config, device_url_or_provider)?,
        provider.unwrap_or(UsageProviderArg::All),
    ))
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
        let snapshot = collect_snapshot(provider);
        let response = http_usage(device_url, &snapshot)?;
        print_api_response("push usage", &response);
        thread::sleep(interval);
    }
}

fn parse_provider_arg(value: &str) -> Option<UsageProviderArg> {
    match value.to_ascii_lowercase().as_str() {
        "all" => Some(UsageProviderArg::All),
        "codex" => Some(UsageProviderArg::Codex),
        "claude" => Some(UsageProviderArg::Claude),
        _ => None,
    }
}

fn to_provider_selection(provider: UsageProviderArg) -> ProviderSelection {
    match provider {
        UsageProviderArg::All => ProviderSelection::All,
        UsageProviderArg::Codex => ProviderSelection::Codex,
        UsageProviderArg::Claude => ProviderSelection::Claude,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_usage_accepts_provider_without_device_url() {
        let cli = Cli::try_parse_from(["cli", "push-usage", "codex"]).expect("parse CLI");

        match cli.command {
            Commands::PushUsage {
                device_url_or_provider,
                provider,
            } => {
                assert_eq!(device_url_or_provider, Some("codex".to_string()));
                assert_eq!(provider, None);
            }
            _ => panic!("expected push-usage command"),
        }
    }

    #[test]
    fn push_usage_still_accepts_device_url_then_provider() {
        let cli = Cli::try_parse_from(["cli", "push-usage", "http://192.168.1.50", "claude"])
            .expect("parse CLI");

        match cli.command {
            Commands::PushUsage {
                device_url_or_provider,
                provider,
            } => {
                assert_eq!(
                    device_url_or_provider,
                    Some("http://192.168.1.50".to_string())
                );
                assert_eq!(provider, Some(UsageProviderArg::Claude));
            }
            _ => panic!("expected push-usage command"),
        }
    }
}

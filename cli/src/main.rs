use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use monitor_core::config::{read_config_file, resolve_flash_inputs};
use monitor_core::flash::{flash_firmware, reset_device};
use monitor_core::http::{http_status, http_usage};
use monitor_core::serial::{send_serial, serial_port_names};
use monitor_core::usage::collect_snapshot;
use monitor_core::{ApiResponse, ProviderSelection, SerialRequest, UsageSnapshot};

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
            flash_firmware(&inputs, &cli.port, cli.baud)
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
            device_url,
            provider,
        } => {
            let snapshot = collect_snapshot(to_provider_selection(provider));
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
    for port in serial_port_names()? {
        println!("{port}");
    }
    Ok(())
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
        let snapshot = collect_snapshot(provider);
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

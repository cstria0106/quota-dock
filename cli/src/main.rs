use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand, ValueEnum};
use monitor_core::config::{
    read_config_file, resolve_device_url, resolve_flash_inputs, save_board_ip,
};
use monitor_core::flash::{flash_firmware, reset_device};
use monitor_core::http::{
    http_command, http_status, http_usage, http_usage_provider, postcard_len,
};
use monitor_core::serial::{send_serial, send_serial_status, serial_port_names};
use monitor_core::usage::UsageTheme;
use monitor_core::usage::{attach_provider_images, collect_snapshot, strip_provider_images};
use monitor_core::{
    ApiResponse, DeviceCommand, ProviderSelection, SerialRequest, StatusResponse, UsagePixelArt,
    UsageProvider, UsageProviderUpdate, UsageSnapshot, UsageWindow,
};

#[cfg(windows)]
const DEFAULT_PORT: &str = "COM3";
#[cfg(not(windows))]
const DEFAULT_PORT: &str = "/dev/ttyACM0";
const DEFAULT_BAUD: u32 = 115_200;
const DEFAULT_CONFIG_FILE: &str = "config.toml";
const GAUGE_SWEEP_PATTERN: &[u8] = &[5, 25, 50, 75, 95, 75, 50, 25, 10, 40];

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
    PushTestUsage {
        device_url: Option<String>,
        #[arg(long, value_enum, default_value_t = TestUsageScenario::Mixed)]
        scenario: TestUsageScenario,
        #[arg(long)]
        no_images: bool,
    },
    SmokeTest {
        device_url: Option<String>,
        #[arg(long, value_enum, default_value_t = TestUsageScenario::Mixed)]
        scenario: TestUsageScenario,
        #[arg(long, default_value_t = 5)]
        usage_runs: usize,
        #[arg(long, default_value_t = 6)]
        cycles: usize,
        #[arg(long, default_value_t = 10)]
        gauge_runs: usize,
        #[arg(long, default_value_t = 500)]
        usage_delay_ms: u64,
        #[arg(long, default_value_t = 200)]
        gauge_delay_ms: u64,
        #[arg(long, default_value_t = 250)]
        cycle_delay_ms: u64,
        #[arg(long, default_value_t = 1_000)]
        settle_ms: u64,
        #[arg(long)]
        no_images: bool,
        #[arg(long)]
        repeat_images: bool,
        #[arg(long)]
        full_snapshot: bool,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum TestUsageScenario {
    Mixed,
    Codex,
    Claude,
    Opencode,
    Plain,
    Empty,
    Errors,
    GreenDiagonal,
    GreenChecker,
    BlueFramed,
    OrangeFramed,
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
            let status = send_serial_status(&port, baud, Duration::from_secs(6))?;
            print_status("serial status", &status);
            save_status_ip(&config, &status)?;
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
            let snapshot = collect_usage_snapshot(&config, to_provider_selection(provider), true)?;
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
            let snapshot = collect_usage_snapshot(&config, to_provider_selection(provider), true)?;
            let response = http_usage(&device_url, &snapshot)?;
            print_api_response("push usage", &response);
            Ok(())
        }
        Commands::PushTestUsage {
            device_url,
            scenario,
            no_images,
        } => {
            let device_url = resolve_device_url(&config, device_url)?;
            let snapshot = test_usage_snapshot(scenario, !no_images);
            let response = http_usage(&device_url, &snapshot)?;
            print_api_response("push test usage", &response);
            Ok(())
        }
        Commands::SmokeTest {
            device_url,
            scenario,
            usage_runs,
            cycles,
            gauge_runs,
            usage_delay_ms,
            gauge_delay_ms,
            cycle_delay_ms,
            settle_ms,
            no_images,
            repeat_images,
            full_snapshot,
        } => smoke_test(
            &config,
            device_url,
            SmokeTestOptions {
                scenario,
                include_images: !no_images,
                repeat_images,
                full_snapshot,
                usage_runs,
                cycles,
                gauge_runs,
                usage_delay: Duration::from_millis(usage_delay_ms),
                gauge_delay: Duration::from_millis(gauge_delay_ms),
                cycle_delay: Duration::from_millis(cycle_delay_ms),
                settle: Duration::from_millis(settle_ms),
            },
        ),
        Commands::WatchUsage {
            device_url_or_provider,
            provider,
            interval_secs,
        } => {
            let (device_url, provider) =
                resolve_usage_target(&config, device_url_or_provider, provider)?;
            watch_usage(
                &config,
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

#[derive(Clone, Copy)]
struct SmokeTestOptions {
    scenario: TestUsageScenario,
    include_images: bool,
    repeat_images: bool,
    full_snapshot: bool,
    usage_runs: usize,
    cycles: usize,
    gauge_runs: usize,
    usage_delay: Duration,
    gauge_delay: Duration,
    cycle_delay: Duration,
    settle: Duration,
}

fn smoke_test(
    config: &Path,
    device_url: Option<String>,
    options: SmokeTestOptions,
) -> Result<(), String> {
    let device_url = resolve_device_url(config, device_url)?;
    let usage_runs = options.usage_runs.max(1);
    let cycles = options.cycles.max(1);
    let gauge_runs = options.gauge_runs;
    let mut failures = 0;

    println!("smoke test report");
    println!("  device: {device_url}");
    println!(
        "  scenario: {:?}, images: {}, repeat_images: {}, usage_mode: {}, usage_runs: {}, gauge_runs: {}, cycles: {}",
        options.scenario,
        if options.include_images { "on" } else { "off" },
        if options.repeat_images { "on" } else { "off" },
        if options.full_snapshot {
            "full"
        } else {
            "provider"
        },
        usage_runs,
        gauge_runs,
        cycles
    );

    let initial_status = match timed_status("initial status", &device_url) {
        Ok(status) => status,
        Err(err) => {
            println!("result: FAIL");
            return Err(format!("initial status failed: {err}"));
        }
    };

    for run in 1..=usage_runs {
        let include_images = options.include_images && (run == 1 || options.repeat_images);
        let snapshot = test_usage_snapshot(options.scenario, include_images);
        if !push_usage_dispatch(
            &device_url,
            &snapshot,
            &format!("usage {run}/{usage_runs}"),
            Some(if include_images {
                "images=on"
            } else {
                "images=off"
            }),
            options.full_snapshot,
        ) {
            failures += 1;
        }
        thread::sleep(options.usage_delay);
    }

    let gauge_base_time = unix_now();
    for run in 1..=gauge_runs {
        let (primary, week, month) = gauge_percents(run - 1);
        let include_image = options.include_images && run == 1;
        let snapshot = gauge_sweep_snapshot(gauge_base_time, primary, week, month, include_image);
        let expected = format!("expected=5h:{primary}% week:{week}% month:{month}%");
        if !push_usage_dispatch(
            &device_url,
            &snapshot,
            &format!("gauge {run}/{gauge_runs}"),
            Some(expected.as_str()),
            options.full_snapshot,
        ) {
            failures += 1;
        }
        thread::sleep(options.gauge_delay);
    }

    for cycle in 1..=cycles {
        let started_at = Instant::now();
        match http_command(&device_url, &DeviceCommand::CycleUsageProvider) {
            Ok(response) if response.ok => println!(
                "cycle {cycle}/{cycles}: OK {} ms message={}",
                started_at.elapsed().as_millis(),
                response.message
            ),
            Ok(response) => {
                failures += 1;
                println!(
                    "cycle {cycle}/{cycles}: FAIL {} ms message={}",
                    started_at.elapsed().as_millis(),
                    response.message
                );
            }
            Err(err) => {
                failures += 1;
                println!(
                    "cycle {cycle}/{cycles}: FAIL {} ms error={err}",
                    started_at.elapsed().as_millis()
                );
            }
        }
        thread::sleep(options.cycle_delay);
    }

    thread::sleep(options.settle);
    let final_status = match timed_status("final status", &device_url) {
        Ok(status) => status,
        Err(err) => {
            failures += 1;
            println!("final status: FAIL error={err}");
            None
        }
    };

    print_heap_delta(initial_status.as_ref(), final_status.as_ref());
    if failures == 0 {
        println!("result: PASS");
        Ok(())
    } else {
        println!("result: FAIL failures={failures}");
        Err(format!("smoke test failed with {failures} failure(s)"))
    }
}

fn push_usage_dispatch(
    device_url: &str,
    snapshot: &UsageSnapshot,
    label: &str,
    detail: Option<&str>,
    full_snapshot: bool,
) -> bool {
    if full_snapshot {
        return push_usage_step(device_url, snapshot, label, detail);
    }

    let mut ok = true;
    for (index, provider) in snapshot.providers.iter().cloned().enumerate() {
        let update = UsageProviderUpdate {
            provider,
            updated_at: snapshot.updated_at.clone(),
            updated_at_unix: snapshot.updated_at_unix,
        };
        let provider_label = format!(
            "{label} provider {}/{} {}",
            index + 1,
            snapshot.providers.len(),
            update.provider.id
        );
        if !push_provider_step(device_url, &update, &provider_label, detail) {
            ok = false;
        }
    }
    ok
}

fn push_usage_step(
    device_url: &str,
    snapshot: &UsageSnapshot,
    label: &str,
    detail: Option<&str>,
) -> bool {
    let started_at = Instant::now();
    let payload_bytes = postcard_len(snapshot).unwrap_or_default();
    let detail = detail
        .map(|detail| format!(" {detail}"))
        .unwrap_or_default();
    match http_usage(device_url, snapshot) {
        Ok(response) if response.ok => {
            println!(
                "{label}: OK {} ms bytes={}{} message={}",
                started_at.elapsed().as_millis(),
                payload_bytes,
                detail,
                response.message
            );
            true
        }
        Ok(response) => {
            println!(
                "{label}: FAIL {} ms bytes={}{} message={}",
                started_at.elapsed().as_millis(),
                payload_bytes,
                detail,
                response.message
            );
            false
        }
        Err(err) => {
            println!(
                "{label}: FAIL {} ms bytes={}{} error={err}",
                started_at.elapsed().as_millis(),
                payload_bytes,
                detail
            );
            false
        }
    }
}

fn push_provider_step(
    device_url: &str,
    update: &UsageProviderUpdate,
    label: &str,
    detail: Option<&str>,
) -> bool {
    let started_at = Instant::now();
    let payload_bytes = postcard_len(update).unwrap_or_default();
    let detail = detail
        .map(|detail| format!(" {detail}"))
        .unwrap_or_default();
    match http_usage_provider(device_url, update) {
        Ok(response) if response.ok => {
            println!(
                "{label}: OK {} ms bytes={}{} message={}",
                started_at.elapsed().as_millis(),
                payload_bytes,
                detail,
                response.message
            );
            true
        }
        Ok(response) => {
            println!(
                "{label}: FAIL {} ms bytes={}{} message={}",
                started_at.elapsed().as_millis(),
                payload_bytes,
                detail,
                response.message
            );
            false
        }
        Err(err) => {
            println!(
                "{label}: FAIL {} ms bytes={}{} error={err}",
                started_at.elapsed().as_millis(),
                payload_bytes,
                detail
            );
            false
        }
    }
}

fn timed_status(label: &str, device_url: &str) -> Result<Option<StatusResponse>, String> {
    let started_at = Instant::now();
    let status = http_status(device_url)?;
    println!(
        "{label}: OK {} ms mode={} connected={} ip={} heap_free={} heap_internal={} heap_min={}",
        started_at.elapsed().as_millis(),
        status.mode,
        status.connected,
        status.ip.as_deref().unwrap_or("-"),
        status.heap_free,
        status.heap_internal_free,
        status.heap_min_free
    );
    Ok(Some(status))
}

fn print_heap_delta(initial: Option<&StatusResponse>, final_status: Option<&StatusResponse>) {
    let (Some(initial), Some(final_status)) = (initial, final_status) else {
        println!("heap delta: unavailable");
        return;
    };

    println!(
        "heap delta: free={} internal={} min={}",
        heap_delta(initial.heap_free, final_status.heap_free),
        heap_delta(initial.heap_internal_free, final_status.heap_internal_free),
        heap_delta(initial.heap_min_free, final_status.heap_min_free)
    );
}

fn heap_delta(initial: u32, final_value: u32) -> String {
    format!("{:+}", i64::from(final_value) - i64::from(initial))
}

fn save_status_ip(config: &Path, status: &StatusResponse) -> Result<(), String> {
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

fn print_status(label: &str, status: &StatusResponse) {
    println!(
        "{label}: mode={} connected={} ip={} heap_free={} heap_internal={} heap_min={}",
        status.mode,
        status.connected,
        status.ip.as_deref().unwrap_or("-"),
        status.heap_free,
        status.heap_internal_free,
        status.heap_min_free
    );
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
    config: &Path,
    device_url: &str,
    provider: ProviderSelection,
    interval: Duration,
) -> Result<(), String> {
    let mut include_images = true;
    loop {
        let snapshot = collect_usage_snapshot(config, provider, include_images)?;
        let response = http_usage(device_url, &snapshot)?;
        print_api_response("push usage", &response);
        include_images = false;
        thread::sleep(interval);
    }
}

fn collect_usage_snapshot(
    config_path: &Path,
    provider: ProviderSelection,
    include_images: bool,
) -> Result<UsageSnapshot, String> {
    let mut snapshot = collect_snapshot(provider);
    if !include_images {
        strip_provider_images(&mut snapshot);
        return Ok(snapshot);
    }

    if !config_path.is_file() {
        return Ok(snapshot);
    }
    let Some(usage_config) = read_config_file(config_path)?.usage else {
        return Ok(snapshot);
    };
    if usage_config.images.is_empty() {
        return Ok(snapshot);
    }
    attach_provider_images(&mut snapshot, &usage_config.images, config_path)?;
    Ok(snapshot)
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

fn gauge_percents(step: usize) -> (u8, u8, u8) {
    let primary = GAUGE_SWEEP_PATTERN[step % GAUGE_SWEEP_PATTERN.len()];
    let week = 100_u8.saturating_sub(primary);
    let month = ((u16::from(primary) + 35) % 101) as u8;
    (primary, week, month)
}

fn gauge_sweep_snapshot(
    now: u64,
    primary: u8,
    week: u8,
    month: u8,
    include_image: bool,
) -> UsageSnapshot {
    UsageSnapshot {
        providers: vec![test_provider(
            "GAUGE",
            "GAUGE",
            test_opencode_theme(),
            include_image.then(|| diagonal_art("#18A77A", "#9AF0C8", 96)),
            vec![
                test_window("5h", "5h", primary, now + 2 * 3_600 + 22 * 60),
                test_window("7d", "Week", week, now + 6 * 86_400 + 19 * 3_600),
                test_window("month", "Month", month, now + 22 * 86_400 + 12 * 3_600),
            ],
        )],
        updated_at: format!("GAUGE TEST {now}"),
        updated_at_unix: now,
    }
}

fn test_usage_snapshot(scenario: TestUsageScenario, include_images: bool) -> UsageSnapshot {
    let now = unix_now();
    let providers = match scenario {
        TestUsageScenario::Mixed => vec![
            test_codex_provider(now, include_images),
            test_claude_provider(now, include_images),
            test_opencode_provider(now, include_images),
            test_plain_provider(now),
        ],
        TestUsageScenario::Codex => vec![test_codex_provider(now, include_images)],
        TestUsageScenario::Claude => vec![test_claude_provider(now, include_images)],
        TestUsageScenario::Opencode => vec![test_opencode_provider(now, include_images)],
        TestUsageScenario::Plain => vec![test_plain_provider(now)],
        TestUsageScenario::Empty => vec![
            test_unavailable_provider("EMPTY", "EMPTY"),
            test_unavailable_provider("NO_WINDOWS", "NO WINDOWS"),
        ],
        TestUsageScenario::Errors => vec![
            test_error_provider("CODEX", "CODEX", test_codex_theme()),
            test_error_provider("CLAUDE", "CLAUDE", test_claude_theme()),
        ],
        TestUsageScenario::GreenDiagonal => vec![test_pattern_provider(
            now,
            "GREEN_DIAGONAL",
            "GREEN DIAGONAL",
            test_opencode_theme(),
            include_images.then(|| diagonal_art("#18A77A", "#9AF0C8", 96)),
        )],
        TestUsageScenario::GreenChecker => vec![test_pattern_provider(
            now,
            "GREEN_CHECKER",
            "GREEN CHECKER",
            test_opencode_theme(),
            include_images.then(|| checker_art("#18A77A", "#9AF0C8", 96)),
        )],
        TestUsageScenario::BlueFramed => vec![test_pattern_provider(
            now,
            "BLUE_FRAMED",
            "BLUE FRAMED",
            test_codex_theme(),
            include_images.then(|| framed_art("#3B82F6", "#93C5FD", 96)),
        )],
        TestUsageScenario::OrangeFramed => vec![test_pattern_provider(
            now,
            "ORANGE_FRAMED",
            "ORANGE FRAMED",
            test_claude_theme(),
            include_images.then(|| framed_art("#D97757", "#F3B49D", 96)),
        )],
    };

    UsageSnapshot {
        providers,
        updated_at: format!("TEST {now}"),
        updated_at_unix: now,
    }
}

fn test_codex_provider(now: u64, include_image: bool) -> UsageProvider {
    test_provider(
        "CODEX",
        "CODEX",
        test_codex_theme(),
        include_image.then(|| diagonal_art("#3B82F6", "#93C5FD", 96)),
        vec![
            test_window("5h", "5h", 29, now + 2 * 3_600 + 22 * 60),
            test_window("7d", "Week", 4, now + 6 * 86_400 + 19 * 3_600),
        ],
    )
}

fn test_claude_provider(now: u64, include_image: bool) -> UsageProvider {
    test_provider(
        "CLAUDE",
        "CLAUDE",
        test_claude_theme(),
        include_image.then(|| checker_art("#D97757", "#F3B49D", 96)),
        vec![
            test_window("5h", "5h", 61, now + 53 * 60),
            test_window("7d", "Week", 18, now + 4 * 86_400 + 5 * 3_600),
            test_window("7d-opus", "Opus", 6, now + 4 * 86_400 + 5 * 3_600),
        ],
    )
}

fn test_opencode_provider(now: u64, include_image: bool) -> UsageProvider {
    test_provider(
        "OPENCODE",
        "OPENCODE",
        test_opencode_theme(),
        include_image.then(|| framed_art("#18A77A", "#9AF0C8", 96)),
        vec![
            test_window("5h", "5h", 36, now + 3 * 3_600 + 11 * 60),
            test_window("7d", "Week", 22, now + 5 * 86_400 + 8 * 3_600),
            test_window("month", "Month", 73, now + 22 * 86_400 + 12 * 3_600),
        ],
    )
}

fn test_plain_provider(now: u64) -> UsageProvider {
    test_provider(
        "PLAIN",
        "PLAIN",
        test_plain_theme(),
        None,
        vec![
            test_window("5h", "5h", 42, now + 1_900),
            test_window("7d", "Week", 13, now + 7 * 86_400),
        ],
    )
}

fn test_pattern_provider(
    now: u64,
    id: &str,
    label: &str,
    theme: UsageTheme,
    pixel_art: Option<UsagePixelArt>,
) -> UsageProvider {
    test_provider(
        id,
        label,
        theme,
        pixel_art,
        vec![
            test_window("5h", "5h", 29, now + 2 * 3_600 + 22 * 60),
            test_window("7d", "Week", 4, now + 6 * 86_400 + 19 * 3_600),
        ],
    )
}

fn test_provider(
    id: &str,
    label: &str,
    theme: UsageTheme,
    pixel_art: Option<UsagePixelArt>,
    windows: Vec<UsageWindow>,
) -> UsageProvider {
    UsageProvider {
        id: id.to_string(),
        label: label.to_string(),
        theme_color: Some(theme.accent.clone()),
        theme: Some(theme),
        pixel_art,
        source: "test".to_string(),
        account: Some("sample".to_string()),
        plan: Some("TEST".to_string()),
        windows,
    }
}

fn test_unavailable_provider(id: &str, label: &str) -> UsageProvider {
    UsageProvider {
        id: id.to_string(),
        label: label.to_string(),
        theme_color: None,
        theme: None,
        pixel_art: None,
        source: "unavailable".to_string(),
        account: None,
        plan: None,
        windows: Vec::new(),
    }
}

fn test_error_provider(id: &str, label: &str, theme: UsageTheme) -> UsageProvider {
    test_provider(
        id,
        label,
        theme,
        None,
        vec![UsageWindow {
            kind: "5h".to_string(),
            label: "5h".to_string(),
            used_percent: 0,
            resets_at: None,
            resets_at_unix: None,
            status: "error".to_string(),
        }],
    )
}

fn test_window(kind: &str, label: &str, used_percent: u8, resets_at_unix: u64) -> UsageWindow {
    UsageWindow {
        kind: kind.to_string(),
        label: label.to_string(),
        used_percent,
        resets_at: Some(format!("unix:{resets_at_unix}")),
        resets_at_unix: Some(resets_at_unix),
        status: "live".to_string(),
    }
}

fn test_codex_theme() -> UsageTheme {
    test_theme(
        "#3B82F6", "#101823", "#162338", "#111C2D", "#1A3154", "#263141", "#263246",
    )
}

fn test_claude_theme() -> UsageTheme {
    test_theme(
        "#D97757", "#1D1714", "#2A1E18", "#231912", "#3A251A", "#3A2B25", "#3B2B25",
    )
}

fn test_opencode_theme() -> UsageTheme {
    test_theme(
        "#18A77A", "#101B17", "#172820", "#112119", "#1A3A2C", "#25352F", "#243A31",
    )
}

fn test_plain_theme() -> UsageTheme {
    test_theme(
        "#7C8CA5", "#141820", "#1B202B", "#151C28", "#202838", "#2A303B", "#2D3442",
    )
}

fn test_theme(
    accent: &str,
    panel: &str,
    panel_soft: &str,
    primary_panel: &str,
    primary_panel_soft: &str,
    track: &str,
    pill: &str,
) -> UsageTheme {
    UsageTheme {
        accent: accent.to_string(),
        panel: panel.to_string(),
        panel_soft: panel_soft.to_string(),
        primary_panel: primary_panel.to_string(),
        primary_panel_soft: primary_panel_soft.to_string(),
        track: track.to_string(),
        pill: pill.to_string(),
    }
}

fn diagonal_art(primary: &str, secondary: &str, size: usize) -> UsagePixelArt {
    palette_art(primary, secondary, size, |x, y| {
        if (x + y) % 18 < 9 { '1' } else { '2' }
    })
}

fn checker_art(primary: &str, secondary: &str, size: usize) -> UsagePixelArt {
    palette_art(primary, secondary, size, |x, y| {
        if (x / 12 + y / 12) % 2 == 0 { '1' } else { '2' }
    })
}

fn framed_art(primary: &str, secondary: &str, size: usize) -> UsagePixelArt {
    palette_art(primary, secondary, size, |x, y| {
        if x < 8 || y < 8 || x + 8 >= size || y + 8 >= size {
            '2'
        } else {
            '1'
        }
    })
}

fn palette_art(
    primary: &str,
    secondary: &str,
    size: usize,
    cell: impl Fn(usize, usize) -> char,
) -> UsagePixelArt {
    let rows = (0..size)
        .map(|y| (0..size).map(|x| cell(x, y)).collect())
        .collect();

    UsagePixelArt {
        palette: vec![primary.to_string(), secondary.to_string()],
        rows,
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
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

    #[test]
    fn push_test_usage_accepts_scenario_and_no_images() {
        let cli = Cli::try_parse_from([
            "cli",
            "push-test-usage",
            "http://192.168.1.50",
            "--scenario",
            "opencode",
            "--no-images",
        ])
        .expect("parse CLI");

        match cli.command {
            Commands::PushTestUsage {
                device_url,
                scenario,
                no_images,
            } => {
                assert_eq!(device_url, Some("http://192.168.1.50".to_string()));
                assert_eq!(scenario, TestUsageScenario::Opencode);
                assert!(no_images);
            }
            _ => panic!("expected push-test-usage command"),
        }
    }

    #[test]
    fn smoke_test_accepts_gauge_options() {
        let cli = Cli::try_parse_from([
            "cli",
            "smoke-test",
            "http://192.168.1.50",
            "--usage-runs",
            "2",
            "--gauge-runs",
            "4",
            "--cycles",
            "3",
            "--gauge-delay-ms",
            "50",
        ])
        .expect("parse CLI");

        match cli.command {
            Commands::SmokeTest {
                device_url,
                usage_runs,
                gauge_runs,
                cycles,
                gauge_delay_ms,
                ..
            } => {
                assert_eq!(device_url, Some("http://192.168.1.50".to_string()));
                assert_eq!(usage_runs, 2);
                assert_eq!(gauge_runs, 4);
                assert_eq!(cycles, 3);
                assert_eq!(gauge_delay_ms, 50);
            }
            _ => panic!("expected smoke-test command"),
        }
    }

    #[test]
    fn gauge_sweep_moves_up_and_down() {
        assert_eq!(gauge_percents(0), (5, 95, 40));
        assert_eq!(gauge_percents(4), (95, 5, 29));
        assert_eq!(gauge_percents(7), (25, 75, 60));
    }

    #[test]
    fn test_usage_mixed_includes_multiple_shapes() {
        let snapshot = test_usage_snapshot(TestUsageScenario::Mixed, true);

        assert_eq!(snapshot.providers.len(), 4);
        assert_eq!(snapshot.providers[0].windows.len(), 2);
        assert_eq!(snapshot.providers[1].windows.len(), 3);
        assert_eq!(snapshot.providers[2].windows.len(), 3);
        assert!(
            snapshot
                .providers
                .iter()
                .any(|provider| provider.pixel_art.is_none())
        );
    }
}

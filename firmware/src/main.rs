mod app;
mod drivers;
mod network;
mod time;

use std::thread;
use std::time::Duration;

use app::usage::{draw_usage_snapshot, draw_waiting};
use drivers::display::{disable_panel, EspResult, Sh8601};
use drivers::touch::Ft3168;
use network::{AppCommand, UsageSnapshot};
use time::sleep_ms;

const DISPLAY_ENABLED: bool = true;

fn main() {
    esp_idf_sys::link_patches();

    if let Err(err) = run() {
        println!("ESP-IDF call failed with error {err}");
        unsafe { esp_idf_sys::esp_restart() };
    }
}

fn run() -> EspResult {
    let commands = network::start();

    if !DISPLAY_ENABLED {
        disable_panel()?;
        println!("Display disabled; panel reset is held low.");
        run_without_display(commands);
    }

    println!("Initialize QSPI bus and SH8601 panel");
    let panel = Sh8601::new()?;
    panel.set_brightness(255)?;

    println!("Initialize FT3168 touch controller");
    let touch = Ft3168::new()?;
    match touch.read_point()? {
        Some(point) => println!("Touch detected at {}, {}", point.x, point.y),
        None => println!("No touch detected"),
    }

    println!("Draw waiting usage screen");
    draw_waiting(&panel)?;

    let mut current_usage: Option<UsageSnapshot> = None;
    let mut selected_provider = 0;
    let mut was_touching = false;
    let mut touch_error_logged = false;
    loop {
        while let Ok(command) = commands.try_recv() {
            match command {
                AppCommand::Ping => println!("Received ping command"),
                AppCommand::SetBrightness { value } => panel.set_brightness(value)?,
                AppCommand::CycleUsageProvider => {
                    cycle_provider(&panel, &current_usage, &mut selected_provider)?
                }
                AppCommand::UpdateUsage { snapshot } => {
                    selected_provider =
                        selected_provider.min(snapshot.providers.len().saturating_sub(1));
                    draw_usage_snapshot(&panel, &snapshot, selected_provider)?;
                    current_usage = Some(snapshot);
                }
            }
        }

        let touching = match touch.read_point() {
            Ok(point) => {
                touch_error_logged = false;
                point.is_some()
            }
            Err(err) => {
                if !touch_error_logged {
                    println!("Touch read failed with error {err}");
                    touch_error_logged = true;
                }
                false
            }
        };
        if touching && !was_touching {
            println!("Touch cycle provider");
            cycle_provider(&panel, &current_usage, &mut selected_provider)?;
        }
        was_touching = touching;
        sleep_ms(20);
    }
}

fn run_without_display(commands: network::CommandReceiver) -> ! {
    loop {
        while let Ok(command) = commands.try_recv() {
            match command {
                AppCommand::Ping => println!("Received ping command"),
                AppCommand::SetBrightness { value } => {
                    println!("Display disabled; brightness command ignored: {value}")
                }
                AppCommand::CycleUsageProvider => {
                    println!("Display disabled; cycle command ignored")
                }
                AppCommand::UpdateUsage { snapshot } => {
                    println!(
                        "Display disabled; usage update ignored: {} providers",
                        snapshot.providers.len()
                    )
                }
            }
        }

        thread::sleep(Duration::from_millis(100));
    }
}

fn cycle_provider(
    panel: &Sh8601,
    current_usage: &Option<UsageSnapshot>,
    selected_provider: &mut usize,
) -> EspResult {
    let Some(snapshot) = current_usage else {
        draw_waiting(panel)?;
        return Ok(());
    };
    if snapshot.providers.is_empty() {
        draw_waiting(panel)?;
        return Ok(());
    }

    *selected_provider = (*selected_provider + 1) % snapshot.providers.len();
    draw_usage_snapshot(panel, snapshot, *selected_provider)
}

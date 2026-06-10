mod app;
mod drivers;
mod network;
mod time;

use std::thread;
use std::time::Duration;

use app::renderer::{Renderer, Scene};
use app::status::{network_status_scene, waiting_scene};
use app::usage::usage_scene;
use drivers::display::{disable_panel, EspResult, Sh8601};
use drivers::touch::Ft3168;
use network::{AppCommand, NetworkStatus, UsageSnapshot};
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

    println!("Draw initial background");
    let mut renderer = Renderer::new();
    renderer.set_scene(Scene::new());
    renderer.tick(&panel)?;

    let mut current_usage: Option<UsageSnapshot> = None;
    let mut current_network_status: Option<NetworkStatus> = None;
    let mut selected_provider = 0;
    let mut was_touching = false;
    let mut touch_error_logged = false;
    loop {
        while let Ok(command) = commands.try_recv() {
            match command {
                AppCommand::Ping => println!("Received ping command"),
                AppCommand::SetBrightness { value } => panel.set_brightness(value)?,
                AppCommand::CycleUsageProvider => {
                    cycle_provider(&current_usage, &mut selected_provider);
                    refresh_scene(
                        &mut renderer,
                        &current_usage,
                        &current_network_status,
                        selected_provider,
                    );
                }
                AppCommand::NetworkStatus { status } => {
                    current_network_status = Some(status);
                    refresh_scene(
                        &mut renderer,
                        &current_usage,
                        &current_network_status,
                        selected_provider,
                    );
                }
                AppCommand::UpdateUsage { snapshot } => {
                    if snapshot.providers.is_empty() {
                        current_usage = None;
                    } else {
                        selected_provider =
                            selected_provider.min(snapshot.providers.len().saturating_sub(1));
                        current_usage = Some(snapshot);
                    }
                    refresh_scene(
                        &mut renderer,
                        &current_usage,
                        &current_network_status,
                        selected_provider,
                    );
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
            cycle_provider(&current_usage, &mut selected_provider);
            refresh_scene(
                &mut renderer,
                &current_usage,
                &current_network_status,
                selected_provider,
            );
        }
        was_touching = touching;

        renderer.tick(&panel)?;
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
                AppCommand::NetworkStatus { status } => {
                    println!("Display disabled; network status ignored: {status:?}")
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

fn cycle_provider(current_usage: &Option<UsageSnapshot>, selected_provider: &mut usize) {
    let Some(snapshot) = current_usage else {
        return;
    };
    if snapshot.providers.is_empty() {
        return;
    }

    *selected_provider = (*selected_provider + 1) % snapshot.providers.len();
}

fn refresh_scene(
    renderer: &mut Renderer,
    current_usage: &Option<UsageSnapshot>,
    current_network_status: &Option<NetworkStatus>,
    selected_provider: usize,
) {
    renderer.set_scene(current_scene(
        current_usage,
        current_network_status,
        selected_provider,
    ));
}

fn current_scene(
    current_usage: &Option<UsageSnapshot>,
    current_network_status: &Option<NetworkStatus>,
    selected_provider: usize,
) -> Scene {
    if let Some(snapshot) = current_usage {
        return usage_scene(snapshot, selected_provider);
    }
    if let Some(status) = current_network_status {
        return network_status_scene(status);
    }
    waiting_scene()
}

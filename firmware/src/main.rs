mod app;
mod drivers;
mod network;
mod time;

use std::thread;
use std::time::{Duration, Instant};

use app::renderer::{Renderer, Scene};
use app::status::{network_status_scene, waiting_scene};
use app::usage::{
    cache_provider_images, next_provider_index, normalize_selected_provider, usage_scene,
    ProviderImageCache,
};
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
    let mut current_usage_received_at: Option<Instant> = None;
    let mut provider_image_cache = ProviderImageCache::default();
    let mut current_network_status: Option<NetworkStatus> = None;
    let mut selected_provider = 0;
    let mut was_touching = false;
    let mut touch_error_logged = false;
    let mut last_usage_countdown_refresh = Instant::now();
    loop {
        while let Ok(command) = commands.try_recv() {
            match command {
                AppCommand::Ping => println!("Received ping command"),
                AppCommand::SetBrightness { value } => panel.set_brightness(value)?,
                AppCommand::CycleUsageProvider => {
                    if cycle_provider(&current_usage, &mut selected_provider) {
                        refresh_scene(
                            &mut renderer,
                            &current_usage,
                            &provider_image_cache,
                            current_usage_received_at,
                            &current_network_status,
                            selected_provider,
                        );
                    }
                }
                AppCommand::NetworkStatus { status } => {
                    current_network_status = Some(status);
                    refresh_scene(
                        &mut renderer,
                        &current_usage,
                        &provider_image_cache,
                        current_usage_received_at,
                        &current_network_status,
                        selected_provider,
                    );
                }
                AppCommand::UpdateUsage { mut snapshot } => {
                    cache_provider_images(&mut snapshot, &mut provider_image_cache);
                    if let Some(provider) =
                        normalize_selected_provider(&snapshot, selected_provider)
                    {
                        selected_provider = provider;
                        current_usage = Some(snapshot);
                        current_usage_received_at = Some(Instant::now());
                        last_usage_countdown_refresh = Instant::now();
                    } else {
                        current_usage = None;
                        current_usage_received_at = None;
                    }
                    refresh_scene(
                        &mut renderer,
                        &current_usage,
                        &provider_image_cache,
                        current_usage_received_at,
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
            if cycle_provider(&current_usage, &mut selected_provider) {
                println!("Touch cycle provider");
                refresh_scene(
                    &mut renderer,
                    &current_usage,
                    &provider_image_cache,
                    current_usage_received_at,
                    &current_network_status,
                    selected_provider,
                );
            }
        }
        was_touching = touching;

        if current_usage.is_some()
            && last_usage_countdown_refresh.elapsed() >= Duration::from_secs(1)
        {
            refresh_scene(
                &mut renderer,
                &current_usage,
                &provider_image_cache,
                current_usage_received_at,
                &current_network_status,
                selected_provider,
            );
            last_usage_countdown_refresh = Instant::now();
        }

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

fn cycle_provider(current_usage: &Option<UsageSnapshot>, selected_provider: &mut usize) -> bool {
    let Some(snapshot) = current_usage else {
        return false;
    };
    if let Some(next_provider) = next_provider_index(snapshot, *selected_provider) {
        *selected_provider = next_provider;
        return true;
    }
    false
}

fn refresh_scene(
    renderer: &mut Renderer,
    current_usage: &Option<UsageSnapshot>,
    provider_image_cache: &ProviderImageCache,
    current_usage_received_at: Option<Instant>,
    current_network_status: &Option<NetworkStatus>,
    selected_provider: usize,
) {
    renderer.set_scene(current_scene(
        current_usage,
        provider_image_cache,
        current_usage_received_at,
        current_network_status,
        selected_provider,
    ));
}

fn current_scene(
    current_usage: &Option<UsageSnapshot>,
    provider_image_cache: &ProviderImageCache,
    current_usage_received_at: Option<Instant>,
    current_network_status: &Option<NetworkStatus>,
    selected_provider: usize,
) -> Scene {
    if let Some(snapshot) = current_usage {
        let elapsed_since_update_secs = current_usage_received_at
            .map(|instant| instant.elapsed().as_secs())
            .unwrap_or_default();
        return usage_scene(
            snapshot,
            provider_image_cache,
            selected_provider,
            elapsed_since_update_secs,
        );
    }
    if let Some(status) = current_network_status {
        return network_status_scene(status);
    }
    waiting_scene()
}

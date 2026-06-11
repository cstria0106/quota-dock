mod app;
mod drivers;
mod network;
mod time;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use app::renderer::{Renderer, Scene};
use app::status::{network_status_scene, waiting_scene};
use app::text::{self, Language};
use app::usage::{
    cache_provider_images, next_provider_index, normalize_selected_provider, usage_scene,
    ProviderImageCache,
};
use drivers::display::{disable_panel, EspResult, Sh8601};
use drivers::touch::Ft3168;
use network::{AppCommand, NetworkStatus, SyncPayload, UsageProviderUpdate, UsageSnapshot};

const DISPLAY_ENABLED: bool = true;
const MAIN_LOOP_SLEEP_MS: u64 = 8;
const TOUCH_POLL_INTERVAL_MS: u64 = 80;
const TOUCH_ERROR_LOG_INTERVAL_SECS: u64 = 30;
const USAGE_COUNTDOWN_REFRESH_SECS: u64 = 60;

fn main() {
    esp_idf_sys::link_patches();

    if let Err(err) = run() {
        println!("ESP-IDF call failed with error {err}");
        unsafe { esp_idf_sys::esp_restart() };
    }
}

fn run() -> EspResult {
    let provider_image_statuses = Arc::new(Mutex::new(Vec::new()));
    let commands = network::start(provider_image_statuses.clone());

    if !DISPLAY_ENABLED {
        disable_panel()?;
        println!("Display disabled; panel reset is held low.");
        run_without_display(commands);
    }

    println!("Initialize QSPI bus and SH8601 panel");
    let mut panel = Sh8601::new()?;
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
    renderer.tick(&mut panel)?;

    let mut current_usage: Option<UsageSnapshot> = None;
    let mut current_usage_received_at: Option<Instant> = None;
    let mut provider_image_cache = ProviderImageCache::default();
    let mut current_network_status: Option<NetworkStatus> = None;
    let mut selected_provider = 0;
    let mut language = text::DEFAULT_LANGUAGE;
    let mut was_touching = false;
    let mut last_touch_error_logged_at: Option<Instant> = None;
    let mut last_touch_poll = Instant::now();
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
                            language,
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
                        language,
                    );
                }
                AppCommand::UpdateUsage { mut snapshot } => {
                    cache_provider_images(&mut snapshot, &mut provider_image_cache);
                    publish_provider_image_statuses(
                        &provider_image_cache,
                        &provider_image_statuses,
                    );
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
                        language,
                    );
                }
                AppCommand::UpdateUsageProvider { update } => {
                    apply_provider_update(&mut current_usage, &mut provider_image_cache, update);
                    publish_provider_image_statuses(
                        &provider_image_cache,
                        &provider_image_statuses,
                    );
                    if let Some(snapshot) = &current_usage {
                        if let Some(provider) =
                            normalize_selected_provider(snapshot, selected_provider)
                        {
                            selected_provider = provider;
                            current_usage_received_at = Some(Instant::now());
                            last_usage_countdown_refresh = Instant::now();
                        } else {
                            current_usage = None;
                            current_usage_received_at = None;
                        }
                    }
                    refresh_scene(
                        &mut renderer,
                        &current_usage,
                        &provider_image_cache,
                        current_usage_received_at,
                        &current_network_status,
                        selected_provider,
                        language,
                    );
                }
                AppCommand::Sync { payload } => {
                    if let Some(next_language) = Language::from_code(payload.language.as_str()) {
                        language = next_language;
                    }
                    apply_sync(&mut current_usage, &mut provider_image_cache, payload);
                    publish_provider_image_statuses(
                        &provider_image_cache,
                        &provider_image_statuses,
                    );
                    if let Some(snapshot) = &current_usage {
                        if let Some(provider) =
                            normalize_selected_provider(snapshot, selected_provider)
                        {
                            selected_provider = provider;
                            current_usage_received_at = Some(Instant::now());
                            last_usage_countdown_refresh = Instant::now();
                        } else {
                            current_usage = None;
                            current_usage_received_at = None;
                        }
                    }
                    refresh_scene(
                        &mut renderer,
                        &current_usage,
                        &provider_image_cache,
                        current_usage_received_at,
                        &current_network_status,
                        selected_provider,
                        language,
                    );
                }
            }
        }

        if last_touch_poll.elapsed() >= Duration::from_millis(TOUCH_POLL_INTERVAL_MS) {
            last_touch_poll = Instant::now();
            let touching = match touch.read_point() {
                Ok(point) => point.is_some(),
                Err(err) => {
                    if last_touch_error_logged_at
                        .map(|logged_at| {
                            logged_at.elapsed()
                                >= Duration::from_secs(TOUCH_ERROR_LOG_INTERVAL_SECS)
                        })
                        .unwrap_or(true)
                    {
                        println!("Touch read failed with error {err}");
                        last_touch_error_logged_at = Some(Instant::now());
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
                        language,
                    );
                }
            }
            was_touching = touching;
        }

        if current_usage.is_some()
            && last_usage_countdown_refresh.elapsed()
                >= Duration::from_secs(USAGE_COUNTDOWN_REFRESH_SECS)
        {
            refresh_scene(
                &mut renderer,
                &current_usage,
                &provider_image_cache,
                current_usage_received_at,
                &current_network_status,
                selected_provider,
                language,
            );
            last_usage_countdown_refresh = Instant::now();
        }

        renderer.tick(&mut panel)?;
        thread::sleep(Duration::from_millis(MAIN_LOOP_SLEEP_MS));
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
                AppCommand::UpdateUsageProvider { update } => {
                    println!(
                        "Display disabled; usage provider update ignored: {}",
                        update.provider.id
                    )
                }
                AppCommand::Sync { payload } => {
                    println!(
                        "Display disabled; sync ignored: {} visible provider(s)",
                        payload.visible_provider_ids.len()
                    )
                }
            }
        }

        thread::sleep(Duration::from_millis(100));
    }
}

fn publish_provider_image_statuses(
    provider_image_cache: &ProviderImageCache,
    provider_image_statuses: &Arc<Mutex<Vec<network::ProviderImageStatus>>>,
) {
    if let Ok(mut statuses) = provider_image_statuses.lock() {
        *statuses = provider_image_cache.statuses();
    }
}

fn apply_sync(
    current_usage: &mut Option<UsageSnapshot>,
    provider_image_cache: &mut ProviderImageCache,
    payload: SyncPayload,
) {
    let SyncPayload {
        visible_provider_ids,
        providers,
        updated_at,
        updated_at_unix,
        language: _,
    } = payload;

    provider_image_cache.retain_provider_ids(&visible_provider_ids);
    for provider in &providers {
        provider_image_cache.apply_sync_image(provider);
    }

    if visible_provider_ids.is_empty() {
        *current_usage = None;
        return;
    }

    let target = current_usage.get_or_insert_with(|| UsageSnapshot {
        providers: Vec::new(),
        updated_at: updated_at.clone(),
        updated_at_unix,
    });

    target
        .providers
        .retain(|provider| contains_provider_id(&visible_provider_ids, provider.id.as_str()));
    for sync_provider in providers {
        let Some(mut provider) = sync_provider.usage else {
            continue;
        };
        provider.pixel_art = None;
        target.updated_at = updated_at.clone();
        target.updated_at_unix = updated_at_unix;
        if let Some(existing) = target
            .providers
            .iter_mut()
            .find(|existing| existing.id.eq_ignore_ascii_case(provider.id.as_str()))
        {
            *existing = provider;
        } else {
            target.providers.push(provider);
        }
    }

    target.providers.sort_by_key(|provider| {
        visible_provider_ids
            .iter()
            .position(|provider_id| provider_id.eq_ignore_ascii_case(provider.id.as_str()))
            .unwrap_or(usize::MAX)
    });
}

fn contains_provider_id(provider_ids: &[String], provider_id: &str) -> bool {
    provider_ids
        .iter()
        .any(|visible_id| visible_id.eq_ignore_ascii_case(provider_id))
}

fn apply_provider_update(
    current_usage: &mut Option<UsageSnapshot>,
    provider_image_cache: &mut ProviderImageCache,
    update: UsageProviderUpdate,
) {
    let mut snapshot = UsageSnapshot {
        providers: vec![update.provider],
        updated_at: update.updated_at,
        updated_at_unix: update.updated_at_unix,
    };
    cache_provider_images(&mut snapshot, provider_image_cache);
    let Some(provider) = snapshot.providers.pop() else {
        return;
    };

    let target = current_usage.get_or_insert_with(|| UsageSnapshot {
        providers: Vec::new(),
        updated_at: snapshot.updated_at.clone(),
        updated_at_unix: snapshot.updated_at_unix,
    });
    target.updated_at = snapshot.updated_at;
    target.updated_at_unix = snapshot.updated_at_unix;

    if let Some(existing) = target
        .providers
        .iter_mut()
        .find(|existing| existing.id.eq_ignore_ascii_case(provider.id.as_str()))
    {
        *existing = provider;
    } else {
        target.providers.push(provider);
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
    language: Language,
) {
    renderer.set_scene(current_scene(
        current_usage,
        provider_image_cache,
        current_usage_received_at,
        current_network_status,
        selected_provider,
        language,
    ));
}

fn current_scene(
    current_usage: &Option<UsageSnapshot>,
    provider_image_cache: &ProviderImageCache,
    current_usage_received_at: Option<Instant>,
    current_network_status: &Option<NetworkStatus>,
    selected_provider: usize,
    language: Language,
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
            language,
        );
    }
    if let Some(status) = current_network_status {
        return network_status_scene(status, language);
    }
    waiting_scene(language)
}

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use quota_dock_core::http::{http_command, http_status, http_sync, postcard_len};
use quota_dock_core::serial::{send_serial, send_serial_status, serial_port_names};
use quota_dock_core::{
    attach_provider_images, collect_snapshot, provider_image_id, validate_provider_image,
    ApiResponse, DeviceCommand, ProviderSelection, ProviderSync, SerialRequest, StatusResponse,
    SyncPayload, UsagePixelArt, UsageSnapshot,
};

use crate::firmware::flash_bundled_firmware;

const SERIAL_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug)]
pub enum Task {
    RefreshPorts,
    FlashFirmware {
        port: String,
        baud: u32,
    },
    SendWifi {
        port: String,
        baud: u32,
        ssid: String,
        password: String,
    },
    ClearWifi {
        port: String,
        baud: u32,
    },
    SerialStatus {
        port: String,
        baud: u32,
    },
    HttpStatus {
        device_url: String,
    },
    Command {
        label: &'static str,
        device_url: String,
        command: DeviceCommand,
    },
    SyncUsage {
        device_url: String,
        disabled_provider_ids: BTreeSet<String>,
        image_paths: BTreeMap<String, PathBuf>,
        force_images: bool,
        clear_image_ids: Vec<String>,
        cached_snapshot: Option<UsageSnapshot>,
        cached_available_providers: Vec<AvailableProvider>,
        refresh_usage: bool,
    },
    ValidateImage {
        provider_id: String,
        path: PathBuf,
    },
}

#[derive(Debug)]
pub enum TaskResult {
    Ports(Result<Vec<String>, String>),
    FlashFirmware(Result<(), String>),
    SendWifi(Result<ApiResponse, String>),
    ClearWifi(Result<ApiResponse, String>),
    SerialStatus(Result<StatusResponse, String>),
    HttpStatus(Result<StatusResponse, String>),
    Command {
        label: &'static str,
        result: Result<ApiResponse, String>,
    },
    SyncUsage(SyncReport),
    ValidateImage {
        provider_id: String,
        path: PathBuf,
        result: Result<(), String>,
    },
}

#[derive(Clone, Debug)]
pub struct SyncReport {
    pub snapshot: UsageSnapshot,
    pub available_providers: Vec<AvailableProvider>,
    pub ok: bool,
    pub sent_images: bool,
    pub cleared_images: Vec<String>,
    pub provider_count: usize,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct AvailableProvider {
    pub id: String,
    pub label: String,
    pub source: String,
    pub plan: Option<String>,
}

pub struct Worker {
    task_tx: Sender<Task>,
    result_rx: Receiver<TaskResult>,
}

impl Worker {
    pub fn new() -> Self {
        let (task_tx, task_rx) = mpsc::channel::<Task>();
        let (result_tx, result_rx) = mpsc::channel::<TaskResult>();
        thread::Builder::new()
            .name("quota-dock-desktop-worker".to_string())
            .spawn(move || {
                while let Ok(task) = task_rx.recv() {
                    let _ = result_tx.send(run_task(task));
                }
            })
            .expect("spawn desktop worker");

        Self { task_tx, result_rx }
    }

    pub fn send(&self, task: Task) -> Result<(), String> {
        self.task_tx
            .send(task)
            .map_err(|err| format!("queue task: {err}"))
    }

    pub fn drain(&self) -> Vec<TaskResult> {
        self.result_rx.try_iter().collect()
    }
}

fn run_task(task: Task) -> TaskResult {
    match task {
        Task::RefreshPorts => TaskResult::Ports(serial_port_names()),
        Task::FlashFirmware { port, baud } => {
            TaskResult::FlashFirmware(flash_bundled_firmware(&port, baud))
        }
        Task::SendWifi {
            port,
            baud,
            ssid,
            password,
        } => TaskResult::SendWifi(send_serial(
            &port,
            baud,
            &SerialRequest::SetWifi { ssid, password },
            SERIAL_REQUEST_TIMEOUT,
        )),
        Task::ClearWifi { port, baud } => TaskResult::ClearWifi(send_serial(
            &port,
            baud,
            &SerialRequest::ClearWifi,
            SERIAL_REQUEST_TIMEOUT,
        )),
        Task::SerialStatus { port, baud } => {
            TaskResult::SerialStatus(send_serial_status(&port, baud, SERIAL_REQUEST_TIMEOUT))
        }
        Task::HttpStatus { device_url } => TaskResult::HttpStatus(http_status(&device_url)),
        Task::Command {
            label,
            device_url,
            command,
        } => TaskResult::Command {
            label,
            result: http_command(&device_url, &command),
        },
        Task::SyncUsage {
            device_url,
            disabled_provider_ids,
            image_paths,
            force_images,
            clear_image_ids,
            cached_snapshot,
            cached_available_providers,
            refresh_usage,
        } => TaskResult::SyncUsage(sync_usage(SyncUsageRequest {
            device_url,
            disabled_provider_ids,
            image_paths,
            force_images,
            clear_image_ids,
            cached_snapshot,
            cached_available_providers,
            refresh_usage,
        })),
        Task::ValidateImage { provider_id, path } => {
            let result = validate_provider_image(&path).map(|_| ());
            TaskResult::ValidateImage {
                provider_id,
                path,
                result,
            }
        }
    }
}

struct SyncUsageRequest {
    device_url: String,
    disabled_provider_ids: BTreeSet<String>,
    image_paths: BTreeMap<String, PathBuf>,
    force_images: bool,
    clear_image_ids: Vec<String>,
    cached_snapshot: Option<UsageSnapshot>,
    cached_available_providers: Vec<AvailableProvider>,
    refresh_usage: bool,
}

fn sync_usage(request: SyncUsageRequest) -> SyncReport {
    let SyncUsageRequest {
        device_url,
        disabled_provider_ids,
        image_paths,
        force_images,
        clear_image_ids,
        cached_snapshot,
        cached_available_providers,
        refresh_usage,
    } = request;
    let (collected_snapshot, available_providers) = match (refresh_usage, cached_snapshot) {
        (false, Some(snapshot)) => (snapshot, cached_available_providers),
        _ => collect_usage_data(),
    };
    let mut snapshot = UsageSnapshot {
        providers: collected_snapshot
            .providers
            .into_iter()
            .filter(is_available_provider)
            .filter(|provider| !disabled_provider_ids.contains(&provider.id.to_ascii_lowercase()))
            .collect(),
        updated_at: collected_snapshot.updated_at,
        updated_at_unix: collected_snapshot.updated_at_unix,
    };
    let mut failures = Vec::new();
    let provider_count = snapshot.providers.len();

    let provider_images = match local_provider_images(&snapshot, &image_paths) {
        Ok(provider_images) => provider_images,
        Err(err) => {
            failures.push(err);
            BTreeMap::new()
        }
    };
    let mut image_provider_ids = BTreeSet::new();
    if force_images {
        image_provider_ids.extend(provider_images.keys().cloned());
    }

    strip_snapshot_images(&mut snapshot);
    let first_payload = sync_payload(&snapshot, &provider_images, true, &image_provider_ids);
    let first_image_count = image_payload_count(&first_payload);
    let first_bytes = postcard_len(&first_payload).unwrap_or_default();
    let mut image_update_count = 0;
    match http_sync(&device_url, &first_payload) {
        Ok(response) if response.ok => {
            image_update_count += first_image_count;
            image_provider_ids = response
                .missing_images
                .into_iter()
                .map(|id| id.to_ascii_lowercase())
                .filter(|id| provider_images.contains_key(id))
                .collect();
        }
        Ok(response) => failures.push(format!(
            "sync rejected {} bytes: {}",
            first_bytes, response.message
        )),
        Err(err) => failures.push(format!("sync failed {} bytes: {err}", first_bytes)),
    };

    if failures.is_empty() && !image_provider_ids.is_empty() {
        let image_payload = sync_payload(&snapshot, &provider_images, false, &image_provider_ids);
        let image_count = image_payload_count(&image_payload);
        let image_bytes = postcard_len(&image_payload).unwrap_or_default();
        match http_sync(&device_url, &image_payload) {
            Ok(response) if response.ok => image_update_count += image_count,
            Ok(response) => failures.push(format!(
                "image sync rejected {} bytes: {}",
                image_bytes, response.message
            )),
            Err(err) => failures.push(format!("image sync failed {} bytes: {err}", image_bytes)),
        }
    }

    let ok = failures.is_empty();
    let message = if ok {
        sync_success_message(
            provider_count,
            available_providers.len(),
            image_update_count,
            clear_image_ids.len(),
        )
    } else {
        failures.join("; ")
    };

    SyncReport {
        snapshot,
        available_providers,
        ok,
        sent_images: force_images || image_update_count > 0 || !clear_image_ids.is_empty(),
        cleared_images: if ok { clear_image_ids } else { Vec::new() },
        provider_count,
        message,
    }
}

fn collect_usage_data() -> (UsageSnapshot, Vec<AvailableProvider>) {
    let collected_snapshot = collect_snapshot(ProviderSelection::All);
    let available_providers = collected_snapshot
        .providers
        .iter()
        .filter(|provider| is_available_provider(provider))
        .map(|provider| AvailableProvider {
            id: provider.id.clone(),
            label: provider.label.clone(),
            source: provider.source.clone(),
            plan: provider.plan.clone(),
        })
        .collect::<Vec<_>>();

    (collected_snapshot, available_providers)
}

fn is_available_provider(provider: &quota_dock_core::UsageProvider) -> bool {
    !provider.source.eq_ignore_ascii_case("unavailable")
        && provider
            .windows
            .iter()
            .any(|window| !window.status.eq_ignore_ascii_case("error"))
}

#[derive(Clone)]
struct LocalProviderImage {
    image_id: u32,
    pixel_art: UsagePixelArt,
}

fn local_provider_images(
    snapshot: &UsageSnapshot,
    image_paths: &BTreeMap<String, PathBuf>,
) -> Result<BTreeMap<String, LocalProviderImage>, String> {
    if image_paths.is_empty() {
        return Ok(BTreeMap::new());
    }

    let mut image_snapshot = snapshot.clone();
    attach_provider_images(&mut image_snapshot, image_paths, Path::new("."))?;
    image_snapshot
        .providers
        .into_iter()
        .filter_map(|provider| {
            provider
                .pixel_art
                .map(|pixel_art| (provider.id, provider.label, pixel_art))
        })
        .map(|(provider_id, provider_label, pixel_art)| {
            let image_id = provider_image_id(&pixel_art)
                .map_err(|err| format!("{provider_label} image id failed: {err}"))?;
            Ok((
                provider_id.to_ascii_lowercase(),
                LocalProviderImage {
                    image_id,
                    pixel_art,
                },
            ))
        })
        .collect()
}

fn sync_payload(
    snapshot: &UsageSnapshot,
    provider_images: &BTreeMap<String, LocalProviderImage>,
    include_usage: bool,
    image_provider_ids: &BTreeSet<String>,
) -> SyncPayload {
    SyncPayload {
        visible_provider_ids: snapshot
            .providers
            .iter()
            .map(|provider| provider.id.clone())
            .collect(),
        providers: snapshot
            .providers
            .iter()
            .map(|provider| {
                let provider_id = provider.id.to_ascii_lowercase();
                let image = provider_images.get(&provider_id);
                ProviderSync {
                    id: provider.id.clone(),
                    usage: include_usage.then(|| provider.clone()),
                    image_id: image.map(|image| image.image_id),
                    pixel_art: image
                        .filter(|_| image_provider_ids.contains(&provider_id))
                        .map(|image| image.pixel_art.clone()),
                }
            })
            .collect(),
        updated_at: snapshot.updated_at.clone(),
        updated_at_unix: snapshot.updated_at_unix,
    }
}

fn strip_snapshot_images(snapshot: &mut UsageSnapshot) {
    for provider in &mut snapshot.providers {
        provider.pixel_art = None;
    }
}

fn image_payload_count(payload: &SyncPayload) -> usize {
    payload
        .providers
        .iter()
        .filter(|provider| provider.pixel_art.is_some())
        .count()
}

fn sync_success_message(
    provider_count: usize,
    available_provider_count: usize,
    image_update_count: usize,
    image_clear_count: usize,
) -> String {
    let mut parts = vec![format!(
        "synced {provider_count}/{available_provider_count} provider(s)"
    )];
    if image_update_count > 0 {
        parts.push(format!("updated {image_update_count} image(s)"));
    }
    if image_clear_count > 0 {
        parts.push(format!("cleared {image_clear_count} image(s)"));
    }
    parts.join(", ")
}

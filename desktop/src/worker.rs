use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use monitor_core::http::{http_command, http_status, http_usage_provider, postcard_len};
use monitor_core::serial::{send_serial, send_serial_status, serial_port_names};
use monitor_core::{
    attach_provider_images, collect_snapshot, strip_provider_images, validate_provider_image,
    ApiResponse, DeviceCommand, ProviderSelection, SerialRequest, StatusResponse, UsagePixelArt,
    UsageProviderUpdate, UsageSnapshot,
};

use crate::firmware::flash_bundled_firmware;

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
        selection: ProviderSelection,
        image_paths: BTreeMap<String, PathBuf>,
        include_images: bool,
        clear_image_ids: Vec<String>,
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
    pub ok: bool,
    pub sent_images: bool,
    pub cleared_images: Vec<String>,
    pub provider_count: usize,
    pub message: String,
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
            .name("monitor-desktop-worker".to_string())
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
            Duration::from_secs(30),
        )),
        Task::ClearWifi { port, baud } => TaskResult::ClearWifi(send_serial(
            &port,
            baud,
            &SerialRequest::ClearWifi,
            Duration::from_secs(30),
        )),
        Task::SerialStatus { port, baud } => {
            TaskResult::SerialStatus(send_serial_status(&port, baud, Duration::from_secs(6)))
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
            selection,
            image_paths,
            include_images,
            clear_image_ids,
        } => TaskResult::SyncUsage(sync_usage(
            &device_url,
            selection,
            &image_paths,
            include_images,
            &clear_image_ids,
        )),
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

fn sync_usage(
    device_url: &str,
    selection: ProviderSelection,
    image_paths: &BTreeMap<String, PathBuf>,
    include_images: bool,
    clear_image_ids: &[String],
) -> SyncReport {
    let mut snapshot = collect_snapshot(selection);
    let mut failures = Vec::new();
    let clear_ids = clear_image_ids
        .iter()
        .map(|id| id.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();

    if include_images && !image_paths.is_empty() {
        if let Err(err) = attach_provider_images(&mut snapshot, image_paths, Path::new(".")) {
            failures.push(err);
        }
    } else {
        strip_provider_images(&mut snapshot);
    }

    for provider in &mut snapshot.providers {
        if clear_ids.contains(&provider.id.to_ascii_lowercase()) {
            provider.pixel_art = Some(clear_provider_image_marker());
        }
    }

    let provider_count = snapshot.providers.len();
    if provider_count == 0 {
        return SyncReport {
            snapshot,
            ok: false,
            sent_images: include_images,
            cleared_images: clear_image_ids.to_vec(),
            provider_count,
            message: "no usage providers were collected".to_string(),
        };
    }

    for provider in snapshot.providers.iter().cloned() {
        let update = UsageProviderUpdate {
            provider,
            updated_at: snapshot.updated_at.clone(),
            updated_at_unix: snapshot.updated_at_unix,
        };
        let bytes = postcard_len(&update).unwrap_or_default();
        match http_usage_provider(device_url, &update) {
            Ok(response) if response.ok => {}
            Ok(response) => failures.push(format!(
                "{} rejected {} bytes: {}",
                update.provider.label, bytes, response.message
            )),
            Err(err) => failures.push(format!(
                "{} failed {} bytes: {err}",
                update.provider.label, bytes
            )),
        }
    }

    let ok = failures.is_empty();
    let message = if ok {
        format!("synced {provider_count} provider(s)")
    } else {
        failures.join("; ")
    };

    SyncReport {
        snapshot,
        ok,
        sent_images: include_images,
        cleared_images: clear_image_ids.to_vec(),
        provider_count,
        message,
    }
}

fn clear_provider_image_marker() -> UsagePixelArt {
    UsagePixelArt {
        palette: Vec::new(),
        rows: Vec::new(),
    }
}

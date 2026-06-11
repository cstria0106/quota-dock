mod actions;
mod dock_ui;
mod results;
mod ui;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;
use quota_dock_core::{StatusResponse, UsageSnapshot};

use crate::firmware::{bundled_firmware, BundledFirmware};
use crate::settings::{load_settings, DesktopSettings};
use crate::sync::SyncScheduler;
use crate::worker::Worker;

pub(super) const SERIAL_BAUD: u32 = 115_200;
pub(super) const WIFI_POLL_WINDOW: Duration = Duration::from_secs(60);
pub(super) const WIFI_POLL_INTERVAL: Duration = Duration::from_secs(2);
pub(super) const PROVIDER_IMAGES: &[(&str, &str)] = &[("codex", "Codex"), ("claude", "Claude")];

pub struct QuotaDockApp {
    settings: DesktopSettings,
    settings_path: PathBuf,
    worker: Worker,
    firmware: BundledFirmware,
    current_step: SetupStep,
    serial_ports: Vec<String>,
    wifi_password: String,
    status: Option<StatusResponse>,
    latest_snapshot: Option<UsageSnapshot>,
    sync_scheduler: SyncScheduler,
    sync_enabled: bool,
    send_images_next_sync: bool,
    pending_image_clears: BTreeSet<String>,
    log: Vec<String>,
    port_scan_running: bool,
    flash_running: bool,
    wifi_running: bool,
    clear_wifi_running: bool,
    serial_status_running: bool,
    http_status_running: bool,
    sync_running: bool,
    command_running: Option<&'static str>,
    validating_images: BTreeSet<String>,
    serial_poll_until: Option<Instant>,
    serial_poll_next: Option<Instant>,
    last_sync_ok: Option<bool>,
    last_sync_message: String,
    save_error: Option<String>,
}

impl QuotaDockApp {
    pub fn new(_: &eframe::CreationContext<'_>) -> Self {
        let (settings, settings_path, settings_error) = load_settings();
        let mut app = Self {
            settings,
            settings_path,
            worker: Worker::new(),
            firmware: bundled_firmware(),
            current_step: SetupStep::Usb,
            serial_ports: Vec::new(),
            wifi_password: String::new(),
            status: None,
            latest_snapshot: None,
            sync_scheduler: SyncScheduler::default(),
            sync_enabled: false,
            send_images_next_sync: true,
            pending_image_clears: BTreeSet::new(),
            log: Vec::new(),
            port_scan_running: false,
            flash_running: false,
            wifi_running: false,
            clear_wifi_running: false,
            serial_status_running: false,
            http_status_running: false,
            sync_running: false,
            command_running: None,
            validating_images: BTreeSet::new(),
            serial_poll_until: None,
            serial_poll_next: None,
            last_sync_ok: None,
            last_sync_message: String::new(),
            save_error: None,
        };
        if let Some(err) = settings_error {
            app.push_log(format!("settings load failed: {err}"));
        }
        app.refresh_ports();
        app
    }
}

impl eframe::App for QuotaDockApp {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        self.process_results();
        self.poll_wifi_status_if_due();
        self.sync_if_due();

        ui.heading("QuotaDock");
        ui.add_space(6.0);
        self.step_nav(ui);
        ui.separator();
        match self.current_step {
            SetupStep::Usb => self.usb_step(ui),
            SetupStep::Firmware => self.firmware_step(ui),
            SetupStep::Wifi => self.wifi_step(ui),
            SetupStep::Device => self.device_step(ui),
            SetupStep::Dock => self.dock_step(ui),
        }
        ui.separator();
        self.activity_log(ui);

        ui.ctx().request_repaint_after(Duration::from_millis(250));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SetupStep {
    Usb,
    Firmware,
    Wifi,
    Device,
    Dock,
}

impl SetupStep {
    fn all() -> [Self; 5] {
        [
            Self::Usb,
            Self::Firmware,
            Self::Wifi,
            Self::Device,
            Self::Dock,
        ]
    }

    fn index(self) -> usize {
        match self {
            Self::Usb => 0,
            Self::Firmware => 1,
            Self::Wifi => 2,
            Self::Device => 3,
            Self::Dock => 4,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Usb => "USB",
            Self::Firmware => "Firmware",
            Self::Wifi => "Wi-Fi",
            Self::Device => "Device",
            Self::Dock => "Dock",
        }
    }
}

fn normalize_device_url(value: &str) -> String {
    let value = value.trim();
    if value.contains("://") {
        value.to_string()
    } else {
        format!("http://{value}")
    }
}

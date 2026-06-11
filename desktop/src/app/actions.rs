use std::path::PathBuf;
use std::time::Instant;

use quota_dock_core::DeviceCommand;

use crate::settings::save_to_path;
use crate::worker::Task;

use super::{normalize_device_url, QuotaDockApp, SERIAL_BAUD};

impl QuotaDockApp {
    pub(super) fn refresh_ports(&mut self) {
        self.port_scan_running = true;
        if !self.queue(Task::RefreshPorts) {
            self.port_scan_running = false;
        }
    }

    pub(super) fn flash_firmware(&mut self) {
        self.flash_running = true;
        if !self.queue(Task::FlashFirmware {
            port: self.settings.serial_port.clone(),
            baud: SERIAL_BAUD,
        }) {
            self.flash_running = false;
        }
    }

    pub(super) fn send_wifi(&mut self) {
        self.wifi_running = true;
        if !self.queue(Task::SendWifi {
            port: self.settings.serial_port.clone(),
            baud: SERIAL_BAUD,
            ssid: self.settings.wifi_ssid.clone(),
            password: self.wifi_password.clone(),
        }) {
            self.wifi_running = false;
        }
    }

    pub(super) fn clear_wifi(&mut self) {
        self.clear_wifi_running = true;
        if !self.queue(Task::ClearWifi {
            port: self.settings.serial_port.clone(),
            baud: SERIAL_BAUD,
        }) {
            self.clear_wifi_running = false;
        }
    }

    pub(super) fn request_serial_status(&mut self) {
        self.serial_status_running = true;
        if !self.queue(Task::SerialStatus {
            port: self.settings.serial_port.clone(),
            baud: SERIAL_BAUD,
        }) {
            self.serial_status_running = false;
        }
    }

    pub(super) fn request_http_status(&mut self) {
        self.http_status_running = true;
        if !self.queue(Task::HttpStatus {
            device_url: normalize_device_url(&self.settings.device_url),
        }) {
            self.http_status_running = false;
        }
    }

    pub(super) fn send_command(&mut self, label: &'static str, command: DeviceCommand) {
        self.command_running = Some(label);
        if !self.queue(Task::Command {
            label,
            device_url: normalize_device_url(&self.settings.device_url),
            command,
        }) {
            self.command_running = None;
        }
    }

    pub(super) fn validate_image(&mut self, provider_id: String, path: PathBuf) {
        self.validating_images.insert(provider_id.clone());
        if !self.queue(Task::ValidateImage { provider_id, path }) {
            self.validating_images.clear();
        }
    }

    pub(super) fn clear_provider_image(&mut self, provider_id: &str) {
        self.settings.images.remove(provider_id);
        self.pending_image_clears.insert(provider_id.to_string());
        self.sync_scheduler.request_send_now();
        self.save_settings();
        self.push_log(format!("{provider_id} image cleared"));
    }

    pub(super) fn set_provider_enabled(&mut self, provider_id: &str, enabled: bool) {
        let provider_id = provider_id.to_ascii_lowercase();
        if enabled {
            self.settings.disabled_provider_ids.remove(&provider_id);
            self.send_images_next_sync = true;
        } else {
            self.settings
                .disabled_provider_ids
                .insert(provider_id.clone());
            if let Some(snapshot) = &mut self.latest_snapshot {
                snapshot
                    .providers
                    .retain(|provider| !provider.id.eq_ignore_ascii_case(provider_id.as_str()));
            }
            self.sync_scheduler.request_send_now();
            self.save_settings();
            return;
        }
        self.sync_scheduler.request_now();
        self.save_settings();
    }

    pub(super) fn poll_wifi_status_if_due(&mut self) {
        let Some(next) = self.serial_poll_next else {
            return;
        };
        if self.serial_status_running || Instant::now() < next {
            return;
        }
        self.serial_poll_next = None;
        self.request_serial_status();
    }

    pub(super) fn sync_if_due(&mut self) {
        if !self.sync_enabled || !self.can_use_http() {
            return;
        }
        let now = Instant::now();
        let Some(decision) = self.sync_scheduler.should_start(
            now,
            self.settings.sync_interval_secs,
            self.last_sync_ok,
            self.latest_snapshot.is_some(),
            self.sync_running,
        ) else {
            return;
        };

        let force_images = self.send_images_next_sync;
        let cached_snapshot = (!decision.refresh_usage)
            .then(|| self.latest_snapshot.clone())
            .flatten();
        let cached_available_providers = if decision.refresh_usage {
            Vec::new()
        } else {
            self.available_providers.clone()
        };
        self.sync_running = true;
        if self.queue(Task::SyncUsage {
            device_url: normalize_device_url(&self.settings.device_url),
            disabled_provider_ids: self.settings.disabled_provider_ids.clone(),
            image_paths: self.settings.images.clone(),
            force_images,
            clear_image_ids: self.pending_image_clears.iter().cloned().collect(),
            cached_snapshot,
            cached_available_providers,
            refresh_usage: decision.refresh_usage,
        }) {
            self.sync_scheduler
                .mark_started(now, decision.refresh_usage);
        } else {
            self.sync_running = false;
        }
    }

    pub(super) fn can_use_serial(&self) -> bool {
        !self.settings.serial_port.trim().is_empty()
            && !self.flash_running
            && !self.wifi_running
            && !self.clear_wifi_running
            && !self.serial_status_running
    }

    pub(super) fn can_send_wifi(&self) -> bool {
        self.can_use_serial()
            && !self.settings.wifi_ssid.trim().is_empty()
            && !self.wifi_password.is_empty()
    }

    pub(super) fn can_use_http(&self) -> bool {
        !self.settings.device_url.trim().is_empty()
            && !self.http_status_running
            && self.command_running.is_none()
    }

    pub(super) fn save_settings(&mut self) {
        self.settings = self.settings.clone().normalized();
        match save_to_path(&self.settings_path, &self.settings) {
            Ok(()) => self.save_error = None,
            Err(err) => self.save_error = Some(err),
        }
    }

    pub(super) fn push_log(&mut self, line: String) {
        self.log.push(line);
        if self.log.len() > 200 {
            self.log.remove(0);
        }
    }

    fn queue(&mut self, task: Task) -> bool {
        match self.worker.send(task) {
            Ok(()) => true,
            Err(err) => {
                self.push_log(err);
                false
            }
        }
    }
}

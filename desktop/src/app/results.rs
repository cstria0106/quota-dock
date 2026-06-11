use std::time::Instant;

use monitor_core::StatusResponse;

use crate::worker::{SyncReport, TaskResult};

use super::{normalize_device_url, MonitorApp, SetupStep, WIFI_POLL_INTERVAL, WIFI_POLL_WINDOW};

impl MonitorApp {
    pub(super) fn process_results(&mut self) {
        for result in self.worker.drain() {
            match result {
                TaskResult::Ports(result) => self.handle_ports(result),
                TaskResult::FlashFirmware(result) => self.handle_flash(result),
                TaskResult::SendWifi(result) => self.handle_send_wifi(result),
                TaskResult::ClearWifi(result) => self.handle_clear_wifi(result),
                TaskResult::SerialStatus(result) => {
                    self.serial_status_running = false;
                    match result {
                        Ok(status) => self.handle_serial_status(status),
                        Err(err) => {
                            self.push_log(format!("serial status failed: {err}"));
                            self.schedule_next_wifi_poll();
                        }
                    }
                }
                TaskResult::HttpStatus(result) => {
                    self.http_status_running = false;
                    match result {
                        Ok(status) => {
                            self.handle_device_status(status);
                            self.sync_enabled = true;
                            self.current_step = SetupStep::Monitor;
                            self.sync_scheduler.request_now();
                        }
                        Err(err) => self.push_log(format!("device status failed: {err}")),
                    }
                }
                TaskResult::Command { label, result } => {
                    self.command_running = None;
                    match result {
                        Ok(response) if response.ok => {
                            self.push_log(format!("{label}: {}", response.message))
                        }
                        Ok(response) => {
                            self.push_log(format!("{label} rejected: {}", response.message))
                        }
                        Err(err) => self.push_log(format!("{label} failed: {err}")),
                    }
                }
                TaskResult::SyncUsage(report) => self.handle_sync_report(report),
                TaskResult::ValidateImage {
                    provider_id,
                    path,
                    result,
                } => {
                    self.validating_images.remove(&provider_id);
                    match result {
                        Ok(()) => {
                            self.settings
                                .images
                                .insert(provider_id.clone(), path.clone());
                            self.pending_image_clears.remove(&provider_id);
                            self.send_images_next_sync = true;
                            self.sync_scheduler.request_now();
                            self.save_settings();
                            self.push_log(format!("{provider_id} image: {}", path.display()));
                        }
                        Err(err) => {
                            self.push_log(format!("{provider_id} image failed: {err}"));
                        }
                    }
                }
            }
        }
    }

    fn handle_ports(&mut self, result: Result<Vec<String>, String>) {
        self.port_scan_running = false;
        match result {
            Ok(ports) => {
                self.serial_ports = ports;
                if self.settings.serial_port.is_empty() {
                    if let Some(port) = self.serial_ports.first() {
                        self.settings.serial_port = port.clone();
                        self.save_settings();
                    }
                }
                self.push_log(format!("ports: {}", self.serial_ports.join(", ")));
            }
            Err(err) => self.push_log(format!("port scan failed: {err}")),
        }
    }

    fn handle_flash(&mut self, result: Result<(), String>) {
        self.flash_running = false;
        match result {
            Ok(()) => {
                self.push_log("flash: complete".to_string());
                self.current_step = SetupStep::Wifi;
            }
            Err(err) => self.push_log(format!("flash failed: {err}")),
        }
    }

    fn handle_send_wifi(&mut self, result: Result<monitor_core::ApiResponse, String>) {
        self.wifi_running = false;
        match result {
            Ok(response) if response.ok => {
                self.push_log(format!("wifi: {}", response.message));
                self.begin_wifi_poll();
            }
            Ok(response) => self.push_log(format!("wifi rejected: {}", response.message)),
            Err(err) => self.push_log(format!("wifi failed: {err}")),
        }
    }

    fn handle_clear_wifi(&mut self, result: Result<monitor_core::ApiResponse, String>) {
        self.clear_wifi_running = false;
        match result {
            Ok(response) if response.ok => {
                self.status = None;
                self.sync_enabled = false;
                self.push_log(format!("clear wifi: {}", response.message));
            }
            Ok(response) => self.push_log(format!("clear wifi rejected: {}", response.message)),
            Err(err) => self.push_log(format!("clear wifi failed: {err}")),
        }
    }

    fn begin_wifi_poll(&mut self) {
        let now = Instant::now();
        self.serial_poll_until = Some(now + WIFI_POLL_WINDOW);
        self.serial_poll_next = Some(now);
    }

    fn schedule_next_wifi_poll(&mut self) {
        if let Some(until) = self.serial_poll_until {
            let now = Instant::now();
            if now < until {
                self.serial_poll_next = Some(now + WIFI_POLL_INTERVAL);
            } else {
                self.serial_poll_until = None;
                self.serial_poll_next = None;
                self.push_log("wifi status polling timed out".to_string());
            }
        }
    }

    fn handle_serial_status(&mut self, status: StatusResponse) {
        self.push_log(format!(
            "serial: connected={} ip={}",
            status.connected,
            status.ip.as_deref().unwrap_or("-")
        ));
        if let Some(ip) = status.ip.as_deref().filter(|ip| !ip.trim().is_empty()) {
            self.settings.device_url = normalize_device_url(ip);
            self.save_settings();
        }
        let connected = status.connected && status.ip.is_some();
        self.status = Some(status);
        if connected {
            self.serial_poll_until = None;
            self.serial_poll_next = None;
            self.current_step = SetupStep::Device;
            self.request_http_status();
        } else {
            self.schedule_next_wifi_poll();
        }
    }

    fn handle_device_status(&mut self, status: StatusResponse) {
        if let Some(ip) = status.ip.as_deref().filter(|ip| !ip.trim().is_empty()) {
            self.settings.device_url = normalize_device_url(ip);
            self.save_settings();
        }
        self.push_log(format!(
            "device: connected={} ip={}",
            status.connected,
            status.ip.as_deref().unwrap_or("-")
        ));
        self.status = Some(status);
    }

    fn handle_sync_report(&mut self, report: SyncReport) {
        self.sync_running = false;
        self.latest_snapshot = Some(report.snapshot);
        self.last_sync_ok = Some(report.ok);
        self.last_sync_message = report.message.clone();
        if report.ok {
            if report.sent_images {
                self.send_images_next_sync = false;
            }
            for provider_id in report.cleared_images {
                self.pending_image_clears.remove(&provider_id);
            }
        }
        self.push_log(format!(
            "sync: ok={} providers={} {}",
            report.ok, report.provider_count, report.message
        ));
    }
}

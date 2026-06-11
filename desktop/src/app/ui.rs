use eframe::egui;

use super::{MonitorApp, SetupStep};

impl MonitorApp {
    pub(super) fn step_nav(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for step in SetupStep::all() {
                let label = format!("{}. {}", step.index() + 1, step.label());
                if ui
                    .selectable_label(self.current_step == step, label)
                    .clicked()
                {
                    self.current_step = step;
                }
            }
        });
    }

    pub(super) fn usb_step(&mut self, ui: &mut egui::Ui) {
        ui.heading("USB");
        ui.horizontal(|ui| {
            egui::ComboBox::from_label("Port")
                .selected_text(if self.settings.serial_port.is_empty() {
                    "Select port"
                } else {
                    self.settings.serial_port.as_str()
                })
                .show_ui(ui, |ui| {
                    for port in self.serial_ports.clone() {
                        if ui
                            .selectable_value(
                                &mut self.settings.serial_port,
                                port.clone(),
                                port.as_str(),
                            )
                            .changed()
                        {
                            self.save_settings();
                        }
                    }
                });

            if ui
                .add_enabled(!self.port_scan_running, egui::Button::new("Refresh"))
                .clicked()
            {
                self.refresh_ports();
            }
            if ui
                .add_enabled(self.can_use_serial(), egui::Button::new("USB Status"))
                .clicked()
            {
                self.request_serial_status();
            }
        });

        self.status_summary(ui);
        ui.add_space(8.0);
        if ui
            .add_enabled(self.can_use_serial(), egui::Button::new("Continue"))
            .clicked()
        {
            self.current_step = SetupStep::Firmware;
        }
    }

    pub(super) fn firmware_step(&mut self, ui: &mut egui::Ui) {
        ui.heading("Firmware");
        ui.label(format!(
            "Bundled image: app {} KB, bootloader {} KB, partition {} KB, offset {}",
            self.firmware.app_bytes / 1024,
            self.firmware.bootloader_bytes / 1024,
            self.firmware.partition_table_bytes / 1024,
            self.firmware.offset
        ));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.can_use_serial() && !self.flash_running,
                    egui::Button::new(if self.flash_running {
                        "Flashing..."
                    } else {
                        "Flash Firmware"
                    }),
                )
                .clicked()
            {
                self.flash_firmware();
            }
            if ui
                .add_enabled(!self.flash_running, egui::Button::new("Continue"))
                .clicked()
            {
                self.current_step = SetupStep::Wifi;
            }
        });
    }

    pub(super) fn wifi_step(&mut self, ui: &mut egui::Ui) {
        ui.heading("Wi-Fi");
        if ui
            .horizontal(|ui| {
                ui.label("SSID");
                ui.text_edit_singleline(&mut self.settings.wifi_ssid)
            })
            .inner
            .changed()
        {
            self.save_settings();
        }
        ui.horizontal(|ui| {
            ui.label("Password");
            ui.add(egui::TextEdit::singleline(&mut self.wifi_password).password(true));
        });

        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.can_send_wifi(),
                    egui::Button::new(if self.wifi_running {
                        "Sending..."
                    } else {
                        "Send Wi-Fi"
                    }),
                )
                .clicked()
            {
                self.send_wifi();
            }
            if ui
                .add_enabled(
                    self.can_use_serial() && !self.clear_wifi_running,
                    egui::Button::new("Clear Wi-Fi"),
                )
                .clicked()
            {
                self.clear_wifi();
            }
            if ui
                .add_enabled(self.can_use_serial(), egui::Button::new("USB Status"))
                .clicked()
            {
                self.request_serial_status();
            }
        });

        self.status_summary(ui);
        if ui
            .add_enabled(
                !self.settings.device_url.trim().is_empty(),
                egui::Button::new("Continue"),
            )
            .clicked()
        {
            self.current_step = SetupStep::Device;
        }
    }

    pub(super) fn device_step(&mut self, ui: &mut egui::Ui) {
        ui.heading("Device");
        if ui
            .horizontal(|ui| {
                ui.label("Device URL");
                ui.text_edit_singleline(&mut self.settings.device_url)
            })
            .inner
            .changed()
        {
            self.save_settings();
        }

        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.can_use_http(), egui::Button::new("Test Connection"))
                .clicked()
            {
                self.request_http_status();
            }
            if ui
                .add_enabled(
                    !self.settings.device_url.trim().is_empty() && !self.sync_running,
                    egui::Button::new("Start Sync"),
                )
                .clicked()
            {
                self.sync_enabled = true;
                self.current_step = SetupStep::Monitor;
                self.sync_scheduler.request_now();
            }
        });
        self.status_summary(ui);
    }

    pub(super) fn activity_log(&self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Activity")
            .default_open(true)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(140.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &self.log {
                            ui.monospace(line);
                        }
                    });
            });
    }

    fn status_summary(&self, ui: &mut egui::Ui) {
        if let Some(status) = &self.status {
            ui.label(format!(
                "Status: connected={} ip={} event={}",
                status.connected,
                status.ip.as_deref().unwrap_or("-"),
                status.event.as_deref().unwrap_or("-")
            ));
        }
        if let Some(err) = &self.save_error {
            ui.colored_label(egui::Color32::from_rgb(170, 64, 64), err);
        }
    }
}

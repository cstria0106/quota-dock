use eframe::egui;
use monitor_core::DeviceCommand;

use crate::settings::ProviderChoice;

use super::{MonitorApp, PROVIDER_IMAGES};

impl MonitorApp {
    pub(super) fn monitor_step(&mut self, ui: &mut egui::Ui) {
        ui.heading("Monitor");
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.sync_enabled, "Sync");
            ui.label("Interval");
            let interval = ui.add(
                egui::DragValue::new(&mut self.settings.sync_interval_secs)
                    .range(5..=3_600)
                    .speed(1.0)
                    .suffix(" s"),
            );
            if interval.changed() {
                self.save_settings();
            }
            egui::ComboBox::from_label("Provider")
                .selected_text(self.settings.provider.label())
                .show_ui(ui, |ui| {
                    for provider in ProviderChoice::options() {
                        if ui
                            .selectable_value(
                                &mut self.settings.provider,
                                provider,
                                provider.label(),
                            )
                            .changed()
                        {
                            self.send_images_next_sync = true;
                            self.sync_scheduler.request_now();
                            self.save_settings();
                        }
                    }
                });
            if ui
                .add_enabled(
                    self.can_use_http() && !self.sync_running,
                    egui::Button::new("Sync Now"),
                )
                .clicked()
            {
                self.sync_scheduler.request_now();
            }
        });

        if !self.last_sync_message.is_empty() {
            let color = match self.last_sync_ok {
                Some(true) => egui::Color32::from_rgb(34, 125, 82),
                Some(false) => egui::Color32::from_rgb(170, 64, 64),
                None => ui.visuals().text_color(),
            };
            ui.colored_label(color, self.last_sync_message.as_str());
        }

        ui.add_space(8.0);
        self.usage_snapshot(ui);
        ui.separator();
        self.image_manager(ui);
        ui.separator();
        self.device_controls(ui);
    }

    fn usage_snapshot(&self, ui: &mut egui::Ui) {
        let Some(snapshot) = &self.latest_snapshot else {
            ui.label("No usage snapshot yet.");
            return;
        };
        for provider in &snapshot.providers {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.heading(provider.label.as_str());
                    ui.label(provider.source.as_str());
                    if let Some(plan) = provider.plan.as_deref() {
                        ui.label(plan);
                    }
                });
                for window in &provider.windows {
                    let percent = f32::from(window.used_percent) / 100.0;
                    ui.add(
                        egui::ProgressBar::new(percent)
                            .show_percentage()
                            .text(format!("{} {}", window.label, window.status)),
                    );
                }
            });
        }
    }

    fn image_manager(&mut self, ui: &mut egui::Ui) {
        ui.heading("Images");
        for (provider_id, label) in PROVIDER_IMAGES {
            ui.horizontal(|ui| {
                ui.label(*label);
                let path = self
                    .settings
                    .images
                    .get(*provider_id)
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "-".to_string());
                ui.monospace(path);
                let choose_clicked = ui
                    .add_enabled(
                        !self.validating_images.contains(*provider_id),
                        egui::Button::new("Choose"),
                    )
                    .clicked();
                if choose_clicked {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Images", &["png", "jpg", "jpeg"])
                        .pick_file()
                    {
                        self.validate_image((*provider_id).to_string(), path);
                    }
                }
                if ui
                    .add_enabled(
                        self.settings.images.contains_key(*provider_id),
                        egui::Button::new("Clear"),
                    )
                    .clicked()
                {
                    self.clear_provider_image(provider_id);
                }
            });
        }
    }

    fn device_controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Device");
        ui.horizontal(|ui| {
            ui.label("Brightness");
            let changed = ui
                .add(egui::Slider::new(&mut self.settings.brightness, 0..=255))
                .changed();
            if changed {
                self.save_settings();
            }
            if ui
                .add_enabled(
                    self.can_use_http(),
                    egui::Button::new(if self.command_running == Some("brightness") {
                        "Setting..."
                    } else {
                        "Set"
                    }),
                )
                .clicked()
            {
                self.send_command(
                    "brightness",
                    DeviceCommand::SetBrightness {
                        value: self.settings.brightness,
                    },
                );
            }
        });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.can_use_http(), egui::Button::new("Ping"))
                .clicked()
            {
                self.send_command("ping", DeviceCommand::Ping);
            }
            if ui
                .add_enabled(self.can_use_http(), egui::Button::new("Cycle Provider"))
                .clicked()
            {
                self.send_command("cycle", DeviceCommand::CycleUsageProvider);
            }
        });
    }
}

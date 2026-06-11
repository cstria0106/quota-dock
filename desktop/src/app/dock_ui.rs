use eframe::egui;
use quota_dock_core::DeviceCommand;

use crate::settings::MIN_SYNC_INTERVAL_SECS;

use super::{QuotaDockApp, PROVIDER_IMAGES};

impl QuotaDockApp {
    pub(super) fn dock_step(&mut self, ui: &mut egui::Ui) {
        ui.heading("Dock");
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.sync_enabled, "Sync");
            ui.label("Interval");
            let interval = ui.add(
                egui::DragValue::new(&mut self.settings.sync_interval_secs)
                    .range(MIN_SYNC_INTERVAL_SECS..=3_600)
                    .speed(1.0)
                    .suffix(" s"),
            );
            if interval.changed() {
                self.save_settings();
            }
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
        self.provider_toggles(ui);
        ui.separator();
        self.usage_snapshot(ui);
        ui.separator();
        self.image_manager(ui);
        ui.separator();
        self.device_controls(ui);
    }

    fn provider_toggles(&mut self, ui: &mut egui::Ui) {
        ui.heading("Providers");
        if self.available_providers.is_empty() {
            ui.label("No available providers yet.");
            return;
        }

        for provider in self.available_providers.clone() {
            let provider_id = provider.id.to_ascii_lowercase();
            let mut enabled = !self.settings.disabled_provider_ids.contains(&provider_id);
            ui.horizontal(|ui| {
                if ui.checkbox(&mut enabled, provider.label.as_str()).changed() {
                    self.set_provider_enabled(provider.id.as_str(), enabled);
                }
                ui.label(provider.source.as_str());
                if let Some(plan) = provider.plan.as_deref() {
                    ui.label(plan);
                }
            });
        }
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
        for (provider_id, label) in self.available_provider_image_options() {
            ui.horizontal(|ui| {
                ui.label(label.as_str());
                let path = self
                    .settings
                    .images
                    .get(provider_id.as_str())
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "-".to_string());
                ui.monospace(path);
                let choose_clicked = ui
                    .add_enabled(
                        !self.validating_images.contains(provider_id.as_str()),
                        egui::Button::new("Choose"),
                    )
                    .clicked();
                if choose_clicked {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Images", &["png", "jpg", "jpeg"])
                        .pick_file()
                    {
                        self.validate_image(provider_id.clone(), path);
                    }
                }
                if ui
                    .add_enabled(
                        self.settings.images.contains_key(provider_id.as_str()),
                        egui::Button::new("Clear"),
                    )
                    .clicked()
                {
                    self.clear_provider_image(provider_id.as_str());
                }
            });
        }
    }

    fn available_provider_image_options(&self) -> Vec<(String, String)> {
        self.available_providers
            .iter()
            .filter_map(|provider| {
                PROVIDER_IMAGES
                    .iter()
                    .find(|(provider_id, _)| provider.id.eq_ignore_ascii_case(provider_id))
                    .map(|(_, label)| (provider.id.to_ascii_lowercase(), (*label).to_string()))
            })
            .collect()
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

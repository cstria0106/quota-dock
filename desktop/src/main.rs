use std::time::Duration;

use eframe::egui;
use monitor_core::http::{http_command, http_status};
use monitor_core::serial::{send_serial, serial_port_names};
use monitor_core::{DeviceCommand, SerialRequest};

const DEFAULT_DEVICE_URL: &str = "http://192.168.1.50";
const SERIAL_BAUD: u32 = 115_200;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([760.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Monitor Desktop",
        options,
        Box::new(|_| Ok(Box::<MonitorApp>::default())),
    )
}

#[derive(Default)]
struct MonitorApp {
    serial_port: String,
    wifi_ssid: String,
    wifi_password: String,
    device_url: String,
    brightness: u8,
    log: Vec<String>,
    status: String,
}

impl eframe::App for MonitorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        if self.device_url.is_empty() {
            self.device_url = DEFAULT_DEVICE_URL.to_string();
        }

        ui.heading("Monitor Desktop");
        ui.separator();
        ui.columns(2, |columns| {
            self.serial_setup(&mut columns[0]);
            self.http_controls(&mut columns[1]);
        });

        ui.separator();
        ui.label("Log");
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &self.log {
                    ui.monospace(line);
                }
            });
    }
}

impl MonitorApp {
    fn serial_setup(&mut self, ui: &mut egui::Ui) {
        ui.heading("USB Wi-Fi Setup");

        ui.horizontal(|ui| {
            ui.label("Port");
            ui.text_edit_singleline(&mut self.serial_port);
        });

        if ui.button("Refresh Ports").clicked() {
            match serial_port_names() {
                Ok(ports) => {
                    if self.serial_port.is_empty() {
                        if let Some(port) = ports.first() {
                            self.serial_port = port.clone();
                        }
                    }
                    self.push_log(format!(
                        "ports: {}",
                        ports.into_iter().collect::<Vec<_>>().join(", ")
                    ));
                }
                Err(err) => self.push_log(format!("port scan failed: {err}")),
            }
        }

        ui.horizontal(|ui| {
            ui.label("SSID");
            ui.text_edit_singleline(&mut self.wifi_ssid);
        });
        ui.horizontal(|ui| {
            ui.label("Password");
            ui.add(egui::TextEdit::singleline(&mut self.wifi_password).password(true));
        });

        if ui.button("Send Wi-Fi To ESP32").clicked() {
            let request = SerialRequest::SetWifi {
                ssid: self.wifi_ssid.clone(),
                password: self.wifi_password.clone(),
            };
            self.send_serial(request);
        }

        if ui.button("USB Status").clicked() {
            self.send_serial(SerialRequest::Status);
        }
    }

    fn http_controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Wi-Fi Controls");

        ui.horizontal(|ui| {
            ui.label("Device URL");
            ui.text_edit_singleline(&mut self.device_url);
        });

        if ui.button("Fetch Status").clicked() {
            match http_status(&self.device_url) {
                Ok(status) => {
                    let body = format!(
                        "connected={} ip={}",
                        status.connected,
                        status.ip.as_deref().unwrap_or("-")
                    );
                    self.status = body.clone();
                    self.push_log(format!("status: {body}"));
                }
                Err(err) => self.push_log(format!("status failed: {err}")),
            }
        }

        ui.label(format!("Last status: {}", self.status));

        ui.horizontal(|ui| {
            ui.label("Brightness");
            ui.add(egui::Slider::new(&mut self.brightness, 0..=255));
            if ui.button("Set").clicked() {
                self.send_http_command(DeviceCommand::SetBrightness {
                    value: self.brightness,
                });
            }
        });

        ui.horizontal(|ui| {
            if ui.button("Ping").clicked() {
                self.send_http_command(DeviceCommand::Ping);
            }
            if ui.button("Cycle Provider").clicked() {
                self.send_http_command(DeviceCommand::CycleUsageProvider);
            }
        });
    }

    fn send_serial(&mut self, request: SerialRequest) {
        if self.serial_port.trim().is_empty() {
            self.push_log("serial port is empty".to_string());
            return;
        }

        match send_serial(
            &self.serial_port,
            SERIAL_BAUD,
            &request,
            Duration::from_secs(3),
        ) {
            Ok(response) => self.push_log(format!(
                "serial: ok={} message={}",
                response.ok, response.message
            )),
            Err(err) => self.push_log(format!("serial failed: {err}")),
        }
    }

    fn send_http_command(&mut self, command: DeviceCommand) {
        match http_command(&self.device_url, &command) {
            Ok(response) => self.push_log(format!(
                "command: ok={} message={}",
                response.ok, response.message
            )),
            Err(err) => self.push_log(format!("command failed: {err}")),
        }
    }

    fn push_log(&mut self, line: String) {
        self.log.push(line);
        if self.log.len() > 200 {
            self.log.remove(0);
        }
    }
}

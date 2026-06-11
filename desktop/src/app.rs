use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use quota_dock_core::{DeviceCommand, StatusResponse, UsageSnapshot};
use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

use crate::firmware::{bundled_firmware, BundledFirmware};
use crate::settings::{
    default_usage_window_limit, load_settings, save_to_path, DesktopSettings,
    ProviderDisplaySettings,
};
use crate::sync::SyncScheduler;
use crate::worker::{
    AvailableProvider, BoardDetectionReport, SyncReport, Task, TaskResult, Worker,
};

const SERIAL_BAUD: u32 = 115_200;
const USB_SCAN_INTERVAL: Duration = Duration::from_secs(2);
const FIRMWARE_READY_WINDOW: Duration = Duration::from_secs(30);
const WIFI_POLL_WINDOW: Duration = Duration::from_secs(60);
const WIFI_POLL_INTERVAL: Duration = Duration::from_secs(2);
const HTTP_VERIFY_WINDOW: Duration = Duration::from_secs(20);
const HTTP_VERIFY_INTERVAL: Duration = Duration::from_secs(2);
const UI_TICK_INTERVAL: Duration = Duration::from_millis(250);
const PROVIDER_IMAGES: &[(&str, &str)] = &[("codex", "Codex"), ("claude", "Claude")];
const TRAY_SHOW_ID: &str = "show";
const TRAY_QUIT_ID: &str = "quit";

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .manage(ControllerState::new())
        .invoke_handler(tauri::generate_handler![
            get_app_snapshot,
            scan_usb,
            prepare_initial_setup,
            confirm_flash_firmware,
            send_wifi_credentials,
            cancel_setup,
            sync_now,
            set_sync_enabled,
            set_sync_interval,
            set_device_language,
            set_provider_enabled,
            set_provider_window_slot,
            add_provider_window,
            remove_provider_window,
            set_provider_image_visible,
            set_provider_accent_color,
            reset_provider_accent_color,
            choose_provider_image,
            clear_provider_image,
            provider_image_preview,
            set_brightness,
            set_close_to_tray,
            set_launch_at_startup,
            ping,
            cycle_provider,
        ])
        .setup(|app| {
            setup_tray(app.handle())?;
            setup_close_to_tray(app.handle());
            sync_autostart(app.handle());
            spawn_controller_loop(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("run QuotaDock desktop app");
}

struct ControllerState {
    controller: Mutex<DesktopController>,
}

impl ControllerState {
    fn new() -> Self {
        Self {
            controller: Mutex::new(DesktopController::new()),
        }
    }
}

#[tauri::command]
fn get_app_snapshot(state: State<'_, ControllerState>) -> AppSnapshot {
    let mut controller = state.controller.lock().expect("controller lock");
    controller.tick();
    controller.snapshot()
}

#[tauri::command]
fn scan_usb(state: State<'_, ControllerState>, app: AppHandle) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.prepare_initial_setup(false);
        controller.schedule_usb_scan_now();
        Ok(())
    })
}

#[tauri::command]
fn prepare_initial_setup(
    state: State<'_, ControllerState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.prepare_initial_setup(true);
        Ok(())
    })
}

#[tauri::command]
fn confirm_flash_firmware(
    state: State<'_, ControllerState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.confirm_flash_firmware()
    })
}

#[tauri::command]
fn send_wifi_credentials(
    ssid: String,
    password: String,
    state: State<'_, ControllerState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.send_wifi_credentials(ssid, password)
    })
}

#[tauri::command]
fn cancel_setup(state: State<'_, ControllerState>, app: AppHandle) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.cancel_setup();
        Ok(())
    })
}

#[tauri::command]
fn sync_now(state: State<'_, ControllerState>, app: AppHandle) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.sync_scheduler.request_now();
        Ok(())
    })
}

#[tauri::command]
fn set_sync_enabled(
    enabled: bool,
    state: State<'_, ControllerState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.sync_enabled = enabled;
        if enabled {
            controller.sync_scheduler.request_now();
        }
        Ok(())
    })
}

#[tauri::command]
fn set_sync_interval(
    interval_secs: u64,
    state: State<'_, ControllerState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.settings.sync_interval_secs = interval_secs;
        controller.save_settings();
        Ok(())
    })
}

#[tauri::command]
fn set_device_language(
    language: String,
    state: State<'_, ControllerState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.set_device_language(&language)
    })
}

#[tauri::command]
fn set_provider_enabled(
    provider_id: String,
    enabled: bool,
    state: State<'_, ControllerState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.set_provider_enabled(&provider_id, enabled);
        Ok(())
    })
}

#[tauri::command]
fn set_provider_window_slot(
    provider_id: String,
    slot: usize,
    kind: String,
    state: State<'_, ControllerState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.set_provider_window_slot(&provider_id, slot, &kind);
        Ok(())
    })
}

#[tauri::command]
fn add_provider_window(
    provider_id: String,
    kind: String,
    state: State<'_, ControllerState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.add_provider_window(&provider_id, &kind);
        Ok(())
    })
}

#[tauri::command]
fn remove_provider_window(
    provider_id: String,
    slot: usize,
    state: State<'_, ControllerState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.remove_provider_window(&provider_id, slot);
        Ok(())
    })
}

#[tauri::command]
fn set_provider_image_visible(
    provider_id: String,
    visible: bool,
    state: State<'_, ControllerState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.set_provider_image_visible(&provider_id, visible);
        Ok(())
    })
}

#[tauri::command]
fn set_provider_accent_color(
    provider_id: String,
    color: String,
    state: State<'_, ControllerState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.set_provider_accent_color(&provider_id, &color);
        Ok(())
    })
}

#[tauri::command]
fn reset_provider_accent_color(
    provider_id: String,
    state: State<'_, ControllerState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.reset_provider_accent_color(&provider_id);
        Ok(())
    })
}

#[tauri::command]
fn choose_provider_image(
    provider_id: String,
    state: State<'_, ControllerState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Images", &["png", "jpg", "jpeg"])
        .pick_file()
    else {
        return Ok(snapshot(&state));
    };

    mutate_and_snapshot(&state, &app, |controller| {
        controller.validate_image(provider_id, path);
        Ok(())
    })
}

#[tauri::command]
fn clear_provider_image(
    provider_id: String,
    state: State<'_, ControllerState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.clear_provider_image(&provider_id);
        Ok(())
    })
}

#[tauri::command]
fn provider_image_preview(
    provider_id: String,
    state: State<'_, ControllerState>,
) -> Result<Option<String>, String> {
    let controller = state.controller.lock().expect("controller lock");
    let provider_id = provider_id.to_ascii_lowercase();
    let Some(path) = controller.settings.images.get(provider_id.as_str()) else {
        return Ok(None);
    };
    image_data_url(path).map(Some)
}

#[tauri::command]
fn set_brightness(
    value: u8,
    state: State<'_, ControllerState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.settings.brightness = value;
        controller.save_settings();
        controller.send_command("brightness", DeviceCommand::SetBrightness { value });
        Ok(())
    })
}

#[tauri::command]
fn set_close_to_tray(
    enabled: bool,
    state: State<'_, ControllerState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.settings.close_to_tray = enabled;
        controller.save_settings();
        Ok(())
    })
}

#[tauri::command]
fn set_launch_at_startup(
    enabled: bool,
    state: State<'_, ControllerState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    set_autostart(&app, enabled)?;
    mutate_and_snapshot(&state, &app, |controller| {
        controller.settings.launch_at_startup = enabled;
        controller.save_settings();
        Ok(())
    })
}

#[tauri::command]
fn ping(state: State<'_, ControllerState>, app: AppHandle) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.send_command("ping", DeviceCommand::Ping);
        Ok(())
    })
}

#[tauri::command]
fn cycle_provider(
    state: State<'_, ControllerState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    mutate_and_snapshot(&state, &app, |controller| {
        controller.send_command("cycle", DeviceCommand::CycleUsageProvider);
        Ok(())
    })
}

fn mutate_and_snapshot(
    state: &State<'_, ControllerState>,
    app: &AppHandle,
    mutate: impl FnOnce(&mut DesktopController) -> Result<(), String>,
) -> Result<AppSnapshot, String> {
    let snapshot = {
        let mut controller = state.controller.lock().expect("controller lock");
        mutate(&mut controller)?;
        controller.tick();
        controller.snapshot()
    };
    emit_snapshot(app, &snapshot);
    Ok(snapshot)
}

fn snapshot(state: &State<'_, ControllerState>) -> AppSnapshot {
    let mut controller = state.controller.lock().expect("controller lock");
    controller.tick();
    controller.snapshot()
}

fn emit_snapshot(app: &AppHandle, snapshot: &AppSnapshot) {
    let _ = app.emit("app-snapshot", snapshot);
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, TRAY_SHOW_ID, "QuotaDock 열기", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, TRAY_QUIT_ID, "종료", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let icon = app.default_window_icon().cloned();
    let app_for_click = app.clone();

    let mut tray = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("QuotaDock")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            TRAY_SHOW_ID => show_main_window(app),
            TRAY_QUIT_ID => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(move |_tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => show_main_window(&app_for_click),
            _ => {}
        });

    if let Some(icon) = icon {
        tray = tray.icon(icon);
    }
    tray.build(app)?;
    Ok(())
}

fn setup_close_to_tray(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let app = app.clone();
        window.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = app.state::<ControllerState>();
                let controller = state.controller.lock().expect("controller lock");
                if controller.settings.close_to_tray {
                    api.prevent_close();
                    drop(controller);
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.hide();
                    }
                }
            }
        });
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn sync_autostart(app: &AppHandle) {
    let state = app.state::<ControllerState>();
    let mut controller = state.controller.lock().expect("controller lock");
    let enabled = controller.settings.launch_at_startup;
    if let Err(err) = set_autostart(app, enabled) {
        controller.push_log(format!("autostart sync failed: {err}"));
    }
}

fn set_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager
            .enable()
            .map_err(|err| format!("enable startup launch: {err}"))
    } else {
        manager
            .disable()
            .map_err(|err| format!("disable startup launch: {err}"))
    }
}

fn spawn_controller_loop(app: AppHandle) {
    thread::Builder::new()
        .name("quota-dock-ui-state".to_string())
        .spawn(move || loop {
            thread::sleep(UI_TICK_INTERVAL);
            let snapshot = {
                let state = app.state::<ControllerState>();
                let mut controller = state.controller.lock().expect("controller lock");
                controller.tick();
                controller.snapshot()
            };
            emit_snapshot(&app, &snapshot);
        })
        .expect("spawn desktop controller loop");
}

struct DesktopController {
    settings: DesktopSettings,
    settings_path: PathBuf,
    worker: Worker,
    firmware: BundledFirmware,
    setup: SetupFlow,
    status: Option<StatusResponse>,
    latest_snapshot: Option<UsageSnapshot>,
    available_providers: Vec<AvailableProvider>,
    sync_scheduler: SyncScheduler,
    sync_enabled: bool,
    device_language: String,
    send_images_next_sync: bool,
    pending_image_clears: BTreeSet<String>,
    log: Vec<String>,
    board_probe_running: bool,
    flash_running: bool,
    wifi_running: bool,
    serial_status_running: bool,
    http_status_running: bool,
    sync_running: bool,
    command_running: Option<&'static str>,
    validating_images: BTreeSet<String>,
    last_sync_ok: Option<bool>,
    last_sync_message: String,
    save_error: Option<String>,
    queue_enabled: bool,
}

impl DesktopController {
    fn new() -> Self {
        let (settings, settings_path, settings_error) = load_settings();
        let sync_enabled = !settings.device_url.trim().is_empty();
        let mut controller = Self {
            setup: SetupFlow::new(&settings),
            settings,
            settings_path,
            worker: Worker::new(),
            firmware: bundled_firmware(),
            status: None,
            latest_snapshot: None,
            available_providers: Vec::new(),
            sync_scheduler: SyncScheduler::default(),
            sync_enabled,
            device_language: "ko".to_string(),
            send_images_next_sync: true,
            pending_image_clears: BTreeSet::new(),
            log: Vec::new(),
            board_probe_running: false,
            flash_running: false,
            wifi_running: false,
            serial_status_running: false,
            http_status_running: false,
            sync_running: false,
            command_running: None,
            validating_images: BTreeSet::new(),
            last_sync_ok: None,
            last_sync_message: String::new(),
            save_error: None,
            queue_enabled: true,
        };
        if let Some(err) = settings_error {
            controller.push_log(format!("settings load failed: {err}"));
        }
        if controller.setup.active {
            controller.schedule_usb_scan_now();
        } else {
            controller.sync_scheduler.request_now();
        }
        controller
    }

    fn tick(&mut self) {
        self.process_results();
        self.poll_usb_scan_if_due();
        self.poll_serial_status_if_due();
        self.poll_http_verify_if_due();
        self.sync_if_due();
    }

    fn snapshot(&self) -> AppSnapshot {
        AppSnapshot {
            view: if self.setup.active || self.settings.device_url.trim().is_empty() {
                AppView::Setup
            } else {
                AppView::Dock
            },
            setup: self.setup_snapshot(),
            dock: self.dock_snapshot(),
            log: self.log.clone(),
        }
    }

    fn setup_snapshot(&self) -> SetupSnapshot {
        SetupSnapshot {
            active: self.setup.active,
            stage: self.setup.stage,
            port: self.setup.port.clone(),
            headline: self.setup_headline().to_string(),
            detail: self.setup_detail().to_string(),
            last_error: self.setup.last_error.clone(),
            wifi_ssid: self.settings.wifi_ssid.clone(),
            can_cancel: !self.settings.device_url.trim().is_empty(),
            busy: self.board_probe_running
                || self.flash_running
                || self.wifi_running
                || self.serial_status_running
                || self.http_status_running,
            firmware: FirmwareSnapshot {
                app_kb: self.firmware.app_bytes / 1024,
                bootloader_kb: self.firmware.bootloader_bytes / 1024,
                partition_table_kb: self.firmware.partition_table_bytes / 1024,
                offset: self.firmware.offset.to_string(),
            },
            status: self.status.clone(),
        }
    }

    fn dock_snapshot(&self) -> DockSnapshot {
        DockSnapshot {
            device_url: self.settings.device_url.clone(),
            sync_enabled: self.sync_enabled,
            sync_interval_secs: self.settings.sync_interval_secs,
            sync_running: self.sync_running,
            last_sync_ok: self.last_sync_ok,
            last_sync_message: self.last_sync_message.clone(),
            status: self.status.clone(),
            available_providers: self.provider_options(),
            usage_snapshot: self.latest_snapshot.clone(),
            image_options: self.image_options(),
            brightness: self.settings.brightness,
            close_to_tray: self.settings.close_to_tray,
            launch_at_startup: self.settings.launch_at_startup,
            command_running: self.command_running.map(str::to_string),
            save_error: self.save_error.clone(),
        }
    }

    fn setup_headline(&self) -> &'static str {
        match self.setup.stage {
            SetupStage::Idle => "준비 완료",
            SetupStage::WaitingForBoard => "QuotaDock 보드를 USB로 연결해 주세요",
            SetupStage::CheckingFirmware => "보드 상태를 확인하는 중입니다",
            SetupStage::NeedsFlash => "펌웨어 설치가 필요합니다",
            SetupStage::Flashing => "펌웨어를 설치하는 중입니다",
            SetupStage::Wifi => "Wi-Fi 정보를 입력해 주세요",
            SetupStage::SendingWifi => "Wi-Fi 정보를 보드로 전송하는 중입니다",
            SetupStage::ConnectingWifi => "보드가 Wi-Fi에 연결되는 중입니다",
            SetupStage::VerifyingConnection => "보드 연결을 확인하는 중입니다",
            SetupStage::Complete => "초기 셋업이 완료되었습니다",
        }
    }

    fn setup_detail(&self) -> &'static str {
        match self.setup.stage {
            SetupStage::Idle => "Dock 화면으로 이동합니다.",
            SetupStage::WaitingForBoard => "지원 보드가 감지되면 다음 단계로 자동 이동합니다.",
            SetupStage::CheckingFirmware => "USB 연결을 유지해 주세요.",
            SetupStage::NeedsFlash => "펌웨어 설치 버튼을 누른 뒤 확인하면 설치가 시작됩니다.",
            SetupStage::Flashing => "케이블, 앱, 전원을 그대로 유지해 주세요.",
            SetupStage::Wifi => "보드가 사용할 네트워크 이름과 비밀번호를 입력합니다.",
            SetupStage::SendingWifi => "잠시만 기다려 주세요.",
            SetupStage::ConnectingWifi => "IP 주소가 확인되면 자동으로 저장합니다.",
            SetupStage::VerifyingConnection => "저장한 IP로 QuotaDock 프로토콜을 확인합니다.",
            SetupStage::Complete => "곧 Dock 화면으로 이동합니다.",
        }
    }

    fn prepare_initial_setup(&mut self, force_reconfigure: bool) {
        self.setup = SetupFlow::new_active(force_reconfigure);
        self.sync_enabled = false;
        self.schedule_usb_scan_now();
    }

    fn cancel_setup(&mut self) {
        if self.settings.device_url.trim().is_empty() {
            self.prepare_initial_setup(false);
            return;
        }
        self.setup = SetupFlow::idle();
        self.sync_enabled = true;
        self.sync_scheduler.request_now();
    }

    fn schedule_usb_scan_now(&mut self) {
        self.setup.next_scan = Some(Instant::now());
    }

    fn poll_usb_scan_if_due(&mut self) {
        if !self.setup.active
            || self.setup.stage != SetupStage::WaitingForBoard
            || self.board_probe_running
        {
            return;
        }
        let Some(next_scan) = self.setup.next_scan else {
            return;
        };
        if Instant::now() < next_scan {
            return;
        }
        self.setup.next_scan = None;
        self.board_probe_running = true;
        if !self.queue(Task::DetectBoard { baud: SERIAL_BAUD }) {
            self.board_probe_running = false;
            self.setup.next_scan = Some(Instant::now() + USB_SCAN_INTERVAL);
        }
    }

    fn poll_serial_status_if_due(&mut self) {
        let Some(next) = self.setup.serial_poll_next else {
            return;
        };
        if self.serial_status_running || Instant::now() < next {
            return;
        }
        self.setup.serial_poll_next = None;
        self.request_serial_status();
    }

    fn poll_http_verify_if_due(&mut self) {
        let Some(next) = self.setup.http_poll_next else {
            return;
        };
        if self.http_status_running || Instant::now() < next {
            return;
        }
        self.setup.http_poll_next = None;
        self.request_http_status();
    }

    fn process_results(&mut self) {
        for result in self.worker.drain() {
            match result {
                TaskResult::BoardDetection(result) => self.handle_board_detection(result),
                TaskResult::FlashFirmware(result) => self.handle_flash(result),
                TaskResult::SendWifi(result) => self.handle_send_wifi(result),
                TaskResult::SerialStatus(result) => {
                    self.serial_status_running = false;
                    match result {
                        Ok(status) => self.handle_serial_status(status),
                        Err(err) => {
                            self.push_failure_log("serial status failed", &err);
                            self.schedule_next_serial_poll();
                        }
                    }
                }
                TaskResult::HttpStatus(result) => {
                    self.http_status_running = false;
                    match result {
                        Ok(status) => self.handle_http_status(status),
                        Err(err) => self.handle_http_status_error(err),
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
                            let stored = self.store_provider_image(&provider_id, &path);
                            self.settings
                                .images
                                .insert(provider_id.clone(), stored.clone());
                            self.pending_image_clears.remove(&provider_id);
                            self.send_images_next_sync = true;
                            self.sync_scheduler.request_send_now();
                            self.save_settings();
                            self.push_log(format!("{provider_id} image: {}", stored.display()));
                        }
                        Err(err) => {
                            self.push_log(format!("{provider_id} image failed: {err}"));
                        }
                    }
                }
            }
        }
    }

    fn handle_board_detection(&mut self, result: Result<BoardDetectionReport, String>) {
        self.board_probe_running = false;
        match result {
            Ok(report) => {
                let Some(port) = report.port else {
                    self.setup.stage = SetupStage::WaitingForBoard;
                    self.setup.next_scan = Some(Instant::now() + USB_SCAN_INTERVAL);
                    return;
                };
                self.setup.port = Some(port.clone());
                self.settings.serial_port = port;
                self.save_settings();
                self.push_log(format!("board: {}", self.settings.serial_port.as_str()));

                if let Some(status) = report.firmware_status {
                    self.status = Some(status.clone());
                    if self.save_ip_from_status(&status) && !self.setup.force_reconfigure {
                        self.setup.stage = SetupStage::VerifyingConnection;
                        self.begin_http_verify();
                    } else {
                        self.setup.stage = SetupStage::Wifi;
                    }
                } else {
                    self.setup.stage = SetupStage::NeedsFlash;
                }
            }
            Err(err) => {
                self.setup.stage = SetupStage::WaitingForBoard;
                self.setup.last_error = Some(format!("USB 검색을 다시 시도합니다: {err}"));
                self.setup.next_scan = Some(Instant::now() + USB_SCAN_INTERVAL);
            }
        }
    }

    fn confirm_flash_firmware(&mut self) -> Result<(), String> {
        if self.setup.stage != SetupStage::NeedsFlash {
            return Err("펌웨어 설치가 필요한 상태가 아닙니다.".to_string());
        }
        if self.setup.port.as_deref().unwrap_or("").trim().is_empty() {
            return Err("보드 포트를 찾지 못했습니다.".to_string());
        }
        self.setup.last_error = None;
        self.setup.stage = SetupStage::Flashing;
        self.flash_firmware();
        Ok(())
    }

    fn flash_firmware(&mut self) {
        self.flash_running = true;
        if !self.queue(Task::FlashFirmware {
            port: self.settings.serial_port.clone(),
            baud: SERIAL_BAUD,
        }) {
            self.flash_running = false;
            self.setup.stage = SetupStage::NeedsFlash;
        }
    }

    fn handle_flash(&mut self, result: Result<(), String>) {
        self.flash_running = false;
        match result {
            Ok(()) => {
                self.push_log("flash: complete".to_string());
                self.setup.stage = SetupStage::CheckingFirmware;
                self.begin_serial_poll(SerialPollKind::FirmwareReady, FIRMWARE_READY_WINDOW);
            }
            Err(err) => {
                self.setup.stage = SetupStage::NeedsFlash;
                self.setup.last_error = Some(first_error_line("펌웨어 설치 실패", &err));
                self.push_failure_log("flash failed", &err);
            }
        }
    }

    fn send_wifi_credentials(&mut self, ssid: String, password: String) -> Result<(), String> {
        if self.setup.stage != SetupStage::Wifi {
            return Err("Wi-Fi 정보를 보낼 수 있는 상태가 아닙니다.".to_string());
        }
        if ssid.trim().is_empty() {
            return Err("Wi-Fi 이름을 입력해 주세요.".to_string());
        }
        if password.is_empty() {
            return Err("Wi-Fi 비밀번호를 입력해 주세요.".to_string());
        }
        if !self.can_use_serial() {
            return Err("USB 작업이 진행 중입니다.".to_string());
        }

        self.settings.wifi_ssid = ssid.trim().to_string();
        self.save_settings();
        self.setup.last_error = None;
        self.setup.stage = SetupStage::SendingWifi;
        self.wifi_running = true;
        if !self.queue(Task::SendWifi {
            port: self.settings.serial_port.clone(),
            baud: SERIAL_BAUD,
            ssid: self.settings.wifi_ssid.clone(),
            password,
        }) {
            self.wifi_running = false;
            self.setup.stage = SetupStage::Wifi;
        }
        Ok(())
    }

    fn handle_send_wifi(&mut self, result: Result<quota_dock_core::ApiResponse, String>) {
        self.wifi_running = false;
        match result {
            Ok(response) if response.ok => {
                self.push_log(format!("wifi: {}", response.message));
                self.setup.stage = SetupStage::ConnectingWifi;
                self.begin_serial_poll(SerialPollKind::WifiConnect, WIFI_POLL_WINDOW);
            }
            Ok(response) => {
                self.setup.stage = SetupStage::Wifi;
                self.setup.last_error = Some(format!("Wi-Fi 설정 거부: {}", response.message));
                self.push_log(format!("wifi rejected: {}", response.message));
            }
            Err(err) => {
                self.setup.stage = SetupStage::Wifi;
                self.setup.last_error = Some(first_error_line("Wi-Fi 전송 실패", &err));
                self.push_failure_log("wifi failed", &err);
            }
        }
    }

    fn begin_serial_poll(&mut self, kind: SerialPollKind, window: Duration) {
        let now = Instant::now();
        self.setup.serial_poll_kind = Some(kind);
        self.setup.serial_poll_until = Some(now + window);
        self.setup.serial_poll_next = Some(now + WIFI_POLL_INTERVAL);
    }

    fn schedule_next_serial_poll(&mut self) {
        let Some(kind) = self.setup.serial_poll_kind else {
            return;
        };
        let Some(until) = self.setup.serial_poll_until else {
            return;
        };
        let now = Instant::now();
        if now < until {
            self.setup.serial_poll_next = Some(now + WIFI_POLL_INTERVAL);
            return;
        }

        self.setup.serial_poll_kind = None;
        self.setup.serial_poll_until = None;
        self.setup.serial_poll_next = None;
        match kind {
            SerialPollKind::FirmwareReady => {
                self.setup.stage = SetupStage::NeedsFlash;
                self.setup.last_error =
                    Some("펌웨어 설치 후 보드 응답을 확인하지 못했습니다.".to_string());
            }
            SerialPollKind::WifiConnect => {
                self.setup.stage = SetupStage::Wifi;
                self.setup.last_error =
                    Some("Wi-Fi 연결 후 IP 주소를 확인하지 못했습니다.".to_string());
            }
        }
    }

    fn handle_serial_status(&mut self, status: StatusResponse) {
        self.push_log(format!(
            "serial: connected={} ip={}",
            status.connected,
            status.ip.as_deref().unwrap_or("-")
        ));
        self.status = Some(status.clone());

        match self.setup.serial_poll_kind {
            Some(SerialPollKind::FirmwareReady) => {
                self.stop_serial_poll();
                if self.save_ip_from_status(&status) && !self.setup.force_reconfigure {
                    self.setup.stage = SetupStage::VerifyingConnection;
                    self.begin_http_verify();
                } else {
                    self.setup.stage = SetupStage::Wifi;
                }
            }
            Some(SerialPollKind::WifiConnect) => {
                if self.save_ip_from_status(&status) {
                    self.stop_serial_poll();
                    self.setup.stage = SetupStage::VerifyingConnection;
                    self.begin_http_verify();
                } else {
                    self.schedule_next_serial_poll();
                }
            }
            None => {
                self.save_ip_from_status(&status);
            }
        }
    }

    fn stop_serial_poll(&mut self) {
        self.setup.serial_poll_kind = None;
        self.setup.serial_poll_until = None;
        self.setup.serial_poll_next = None;
    }

    fn save_ip_from_status(&mut self, status: &StatusResponse) -> bool {
        let Some(ip) = status.ip.as_deref().filter(|ip| !ip.trim().is_empty()) else {
            return false;
        };
        if !status.connected {
            return false;
        }
        self.settings.device_url = normalize_device_url(ip);
        self.save_settings();
        true
    }

    fn begin_http_verify(&mut self) {
        let now = Instant::now();
        self.setup.http_poll_until = Some(now + HTTP_VERIFY_WINDOW);
        self.setup.http_poll_next = None;
        self.request_http_status();
    }

    fn schedule_next_http_verify(&mut self, message: String) {
        let Some(until) = self.setup.http_poll_until else {
            self.setup.stage = SetupStage::Wifi;
            self.setup.last_error = Some(message);
            return;
        };
        let now = Instant::now();
        if now < until {
            self.setup.http_poll_next = Some(now + HTTP_VERIFY_INTERVAL);
        } else {
            self.setup.http_poll_until = None;
            self.setup.http_poll_next = None;
            self.setup.stage = SetupStage::Wifi;
            self.setup.last_error = Some(message);
        }
    }

    fn handle_http_status(&mut self, status: StatusResponse) {
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

        if self.setup.active && self.setup.stage == SetupStage::VerifyingConnection {
            self.setup.stage = SetupStage::Complete;
            self.setup = SetupFlow::idle();
        }
        self.sync_enabled = true;
        self.sync_scheduler.request_now();
    }

    fn handle_http_status_error(&mut self, err: String) {
        if self.setup.active && self.setup.stage == SetupStage::VerifyingConnection {
            self.schedule_next_http_verify(first_error_line("보드 연결 확인 실패", &err));
        } else {
            self.push_log(format!("device status failed: {err}"));
        }
    }

    fn request_serial_status(&mut self) {
        self.serial_status_running = true;
        if !self.queue(Task::SerialStatus {
            port: self.settings.serial_port.clone(),
            baud: SERIAL_BAUD,
        }) {
            self.serial_status_running = false;
        }
    }

    fn request_http_status(&mut self) {
        self.http_status_running = true;
        if !self.queue(Task::HttpStatus {
            device_url: normalize_device_url(&self.settings.device_url),
        }) {
            self.http_status_running = false;
        }
    }

    fn send_command(&mut self, label: &'static str, command: DeviceCommand) {
        if !self.can_use_http() {
            self.push_log(format!("{label}: device is busy or missing"));
            return;
        }
        self.command_running = Some(label);
        if !self.queue(Task::Command {
            label,
            device_url: normalize_device_url(&self.settings.device_url),
            command,
        }) {
            self.command_running = None;
        }
    }

    fn validate_image(&mut self, provider_id: String, path: PathBuf) {
        self.validating_images.insert(provider_id.clone());
        if !self.queue(Task::ValidateImage { provider_id, path }) {
            self.validating_images.clear();
        }
    }

    fn images_dir(&self) -> PathBuf {
        self.settings_path
            .parent()
            .map(|parent| parent.join("images"))
            .unwrap_or_else(|| PathBuf::from("images"))
    }

    // Copies the user-selected image into the app's data directory so the
    // provider image survives the original file being moved or deleted.
    // Falls back to the original path if the copy fails.
    fn store_provider_image(&mut self, provider_id: &str, src: &std::path::Path) -> PathBuf {
        let provider_id = provider_id.to_ascii_lowercase();
        let extension = src
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .unwrap_or_else(|| "png".to_string());
        let dir = self.images_dir();
        let dest = dir.join(format!("{provider_id}.{extension}"));
        if let Err(err) = std::fs::create_dir_all(&dir).and_then(|()| std::fs::copy(src, &dest).map(|_| ())) {
            self.push_log(format!(
                "{provider_id} image copy failed ({err}), using original path"
            ));
            return src.to_path_buf();
        }
        dest
    }

    fn clear_provider_image(&mut self, provider_id: &str) {
        if let Some(path) = self.settings.images.remove(provider_id) {
            if path.starts_with(self.images_dir()) {
                let _ = std::fs::remove_file(&path);
            }
        }
        self.pending_image_clears.insert(provider_id.to_string());
        self.sync_scheduler.request_send_now();
        self.save_settings();
        self.push_log(format!("{provider_id} image cleared"));
    }

    fn set_provider_enabled(&mut self, provider_id: &str, enabled: bool) {
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

    fn provider_display_settings_mut(&mut self, provider_id: &str) -> &mut ProviderDisplaySettings {
        self.settings
            .provider_display
            .entry(provider_id.to_ascii_lowercase())
            .or_default()
    }

    fn set_provider_window_slot(&mut self, provider_id: &str, slot: usize, kind: &str) {
        let provider_id = provider_id.to_ascii_lowercase();
        let mut windows = self.provider_selected_windows(provider_id.as_str());
        if slot >= windows.len() || !self.provider_has_window(provider_id.as_str(), kind) {
            return;
        }
        if let Some(selected_slot) = windows.iter().position(|window_kind| window_kind == kind) {
            windows.swap(slot, selected_slot);
        } else {
            windows[slot] = kind.to_string();
        }
        self.provider_display_settings_mut(provider_id.as_str())
            .usage_windows = windows;
        self.sync_scheduler.request_send_now();
        self.save_settings();
    }

    fn add_provider_window(&mut self, provider_id: &str, kind: &str) {
        let provider_id = provider_id.to_ascii_lowercase();
        let mut windows = self.provider_selected_windows(provider_id.as_str());
        if windows.len() >= default_usage_window_limit()
            || windows.iter().any(|window_kind| window_kind == kind)
            || !self.provider_has_window(provider_id.as_str(), kind)
        {
            return;
        }
        windows.push(kind.to_string());
        self.provider_display_settings_mut(provider_id.as_str())
            .usage_windows = windows;
        self.sync_scheduler.request_send_now();
        self.save_settings();
    }

    fn remove_provider_window(&mut self, provider_id: &str, slot: usize) {
        let provider_id = provider_id.to_ascii_lowercase();
        let mut windows = self.provider_selected_windows(provider_id.as_str());
        if windows.len() <= 1 || slot >= windows.len() {
            return;
        }
        windows.remove(slot);
        self.provider_display_settings_mut(provider_id.as_str())
            .usage_windows = windows;
        self.sync_scheduler.request_send_now();
        self.save_settings();
    }

    fn set_provider_image_visible(&mut self, provider_id: &str, visible: bool) {
        self.provider_display_settings_mut(provider_id).show_image = visible;
        self.send_images_next_sync = true;
        self.sync_scheduler.request_send_now();
        self.save_settings();
    }

    fn set_provider_accent_color(&mut self, provider_id: &str, color: &str) {
        if !is_hex_color(color) {
            return;
        }
        self.provider_display_settings_mut(provider_id).accent_color =
            Some(color.trim().to_ascii_uppercase());
        self.sync_scheduler.request_send_now();
        self.save_settings();
    }

    fn reset_provider_accent_color(&mut self, provider_id: &str) {
        self.provider_display_settings_mut(provider_id).accent_color = None;
        self.sync_scheduler.request_send_now();
        self.save_settings();
    }

    fn sync_if_due(&mut self) {
        if self.setup.active || !self.sync_enabled || !self.can_use_http() {
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
            language: self.device_language.clone(),
            disabled_provider_ids: self.settings.disabled_provider_ids.clone(),
            provider_display: self.settings.provider_display.clone(),
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

    fn handle_sync_report(&mut self, report: SyncReport) {
        self.sync_running = false;
        self.available_providers = report.available_providers;
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

    fn set_device_language(&mut self, language: &str) -> Result<(), String> {
        let language = normalize_device_language(language)?;
        if self.device_language == language {
            return Ok(());
        }
        self.device_language = language;
        self.sync_scheduler.request_send_now();
        Ok(())
    }

    fn provider_options(&self) -> Vec<ProviderOptionSnapshot> {
        self.available_providers
            .iter()
            .map(|provider| {
                let provider_id = provider.id.to_ascii_lowercase();
                let display = self.settings.provider_display.get(provider_id.as_str());
                let selected_windows = self.provider_selected_windows(provider_id.as_str());
                let usage_window_limit = selected_windows.len();
                let show_image = display.map(|settings| settings.show_image).unwrap_or(true);
                let accent_color = display
                    .and_then(|settings| settings.accent_color.clone())
                    .or_else(|| provider.accent_color.clone())
                    .or_else(|| {
                        provider_accent_color(self.latest_snapshot.as_ref(), provider.id.as_str())
                    });
                let windows = self
                    .provider_window_options(provider_id.as_str())
                    .into_iter()
                    .filter_map(|kind| {
                        provider
                            .windows
                            .iter()
                            .find(|window| window.kind == kind)
                            .map(|window| ProviderWindowOptionSnapshot {
                                kind: window.kind.clone(),
                                label: window.label.clone(),
                                used_percent: window.used_percent,
                                status: window.status.clone(),
                                enabled: selected_windows.contains(&window.kind),
                            })
                    })
                    .collect();
                ProviderOptionSnapshot {
                    id: provider.id.clone(),
                    label: provider.label.clone(),
                    source: provider.source.clone(),
                    plan: provider.plan.clone(),
                    enabled: !self.settings.disabled_provider_ids.contains(&provider_id),
                    usage_window_limit,
                    show_image,
                    accent_color,
                    image_path: self
                        .settings
                        .images
                        .get(provider_id.as_str())
                        .map(|path| path.display().to_string()),
                    validating_image: self.validating_images.contains(provider_id.as_str()),
                    windows,
                }
            })
            .collect()
    }

    fn provider_selected_windows(&self, provider_id: &str) -> Vec<String> {
        let Some(provider) = self
            .available_providers
            .iter()
            .find(|provider| provider.id.eq_ignore_ascii_case(provider_id))
        else {
            return Vec::new();
        };
        let configured = self
            .settings
            .provider_display
            .get(provider_id)
            .map(|settings| {
                settings
                    .usage_windows
                    .iter()
                    .filter(|kind| provider.windows.iter().any(|window| &window.kind == *kind))
                    .cloned()
                    .take(default_usage_window_limit())
                    .collect::<Vec<_>>()
            });
        configured
            .filter(|windows| !windows.is_empty())
            .unwrap_or_else(|| {
                provider
                    .windows
                    .iter()
                    .take(default_usage_window_limit())
                    .map(|window| window.kind.clone())
                    .collect()
            })
    }

    fn provider_window_options(&self, provider_id: &str) -> Vec<String> {
        let Some(provider) = self
            .available_providers
            .iter()
            .find(|provider| provider.id.eq_ignore_ascii_case(provider_id))
        else {
            return Vec::new();
        };
        let selected = self.provider_selected_windows(provider_id);
        selected
            .iter()
            .cloned()
            .chain(
                provider
                    .windows
                    .iter()
                    .filter(|window| !selected.iter().any(|kind| kind == &window.kind))
                    .map(|window| window.kind.clone()),
            )
            .collect()
    }

    fn provider_has_window(&self, provider_id: &str, kind: &str) -> bool {
        self.available_providers
            .iter()
            .find(|provider| provider.id.eq_ignore_ascii_case(provider_id))
            .is_some_and(|provider| provider.windows.iter().any(|window| window.kind == kind))
    }

    fn image_options(&self) -> Vec<ImageOptionSnapshot> {
        self.available_providers
            .iter()
            .filter_map(|provider| {
                PROVIDER_IMAGES
                    .iter()
                    .find(|(provider_id, _)| provider.id.eq_ignore_ascii_case(provider_id))
                    .map(|(_, label)| {
                        let id = provider.id.to_ascii_lowercase();
                        ImageOptionSnapshot {
                            id: id.clone(),
                            label: (*label).to_string(),
                            path: self
                                .settings
                                .images
                                .get(id.as_str())
                                .map(|path| path.display().to_string()),
                            validating: self.validating_images.contains(id.as_str()),
                        }
                    })
            })
            .collect()
    }

    fn can_use_serial(&self) -> bool {
        !self.settings.serial_port.trim().is_empty()
            && !self.flash_running
            && !self.wifi_running
            && !self.serial_status_running
    }

    fn can_use_http(&self) -> bool {
        !self.settings.device_url.trim().is_empty()
            && !self.http_status_running
            && self.command_running.is_none()
    }

    fn save_settings(&mut self) {
        self.settings = self.settings.clone().normalized();
        match save_to_path(&self.settings_path, &self.settings) {
            Ok(()) => self.save_error = None,
            Err(err) => self.save_error = Some(err),
        }
    }

    fn push_log(&mut self, line: String) {
        self.log.push(line);
        if self.log.len() > 200 {
            self.log.remove(0);
        }
    }

    fn push_failure_log(&mut self, label: &str, message: &str) {
        let mut lines = message.lines();
        if let Some(first) = lines.next() {
            self.push_log(format!("{label}: {first}"));
        } else {
            self.push_log(label.to_string());
        }
        for line in lines {
            self.push_log(format!("  {line}"));
        }
    }

    fn queue(&mut self, task: Task) -> bool {
        if !self.queue_enabled {
            return true;
        }
        match self.worker.send(task) {
            Ok(()) => true,
            Err(err) => {
                self.push_log(err);
                false
            }
        }
    }
}

#[derive(Debug)]
struct SetupFlow {
    active: bool,
    stage: SetupStage,
    port: Option<String>,
    force_reconfigure: bool,
    next_scan: Option<Instant>,
    serial_poll_kind: Option<SerialPollKind>,
    serial_poll_until: Option<Instant>,
    serial_poll_next: Option<Instant>,
    http_poll_until: Option<Instant>,
    http_poll_next: Option<Instant>,
    last_error: Option<String>,
}

impl SetupFlow {
    fn new(settings: &DesktopSettings) -> Self {
        if settings.device_url.trim().is_empty() {
            Self::new_active(false)
        } else {
            Self::idle()
        }
    }

    fn new_active(force_reconfigure: bool) -> Self {
        Self {
            active: true,
            stage: SetupStage::WaitingForBoard,
            port: None,
            force_reconfigure,
            next_scan: Some(Instant::now()),
            serial_poll_kind: None,
            serial_poll_until: None,
            serial_poll_next: None,
            http_poll_until: None,
            http_poll_next: None,
            last_error: None,
        }
    }

    fn idle() -> Self {
        Self {
            active: false,
            stage: SetupStage::Idle,
            port: None,
            force_reconfigure: false,
            next_scan: None,
            serial_poll_kind: None,
            serial_poll_until: None,
            serial_poll_next: None,
            http_poll_until: None,
            http_poll_next: None,
            last_error: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SerialPollKind {
    FirmwareReady,
    WifiConnect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum AppView {
    Setup,
    Dock,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SetupStage {
    Idle,
    WaitingForBoard,
    CheckingFirmware,
    NeedsFlash,
    Flashing,
    Wifi,
    SendingWifi,
    ConnectingWifi,
    VerifyingConnection,
    Complete,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppSnapshot {
    view: AppView,
    setup: SetupSnapshot,
    dock: DockSnapshot,
    log: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupSnapshot {
    active: bool,
    stage: SetupStage,
    port: Option<String>,
    headline: String,
    detail: String,
    last_error: Option<String>,
    wifi_ssid: String,
    can_cancel: bool,
    busy: bool,
    firmware: FirmwareSnapshot,
    status: Option<StatusResponse>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FirmwareSnapshot {
    app_kb: usize,
    bootloader_kb: usize,
    partition_table_kb: usize,
    offset: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DockSnapshot {
    device_url: String,
    sync_enabled: bool,
    sync_interval_secs: u64,
    sync_running: bool,
    last_sync_ok: Option<bool>,
    last_sync_message: String,
    status: Option<StatusResponse>,
    available_providers: Vec<ProviderOptionSnapshot>,
    usage_snapshot: Option<UsageSnapshot>,
    image_options: Vec<ImageOptionSnapshot>,
    brightness: u8,
    close_to_tray: bool,
    launch_at_startup: bool,
    command_running: Option<String>,
    save_error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderOptionSnapshot {
    id: String,
    label: String,
    source: String,
    plan: Option<String>,
    enabled: bool,
    usage_window_limit: usize,
    show_image: bool,
    accent_color: Option<String>,
    image_path: Option<String>,
    validating_image: bool,
    windows: Vec<ProviderWindowOptionSnapshot>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderWindowOptionSnapshot {
    kind: String,
    label: String,
    used_percent: u8,
    status: String,
    enabled: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImageOptionSnapshot {
    id: String,
    label: String,
    path: Option<String>,
    validating: bool,
}

fn normalize_device_url(value: &str) -> String {
    let value = value.trim();
    if value.contains("://") {
        value.to_string()
    } else {
        format!("http://{value}")
    }
}

fn normalize_device_language(value: &str) -> Result<String, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "en" | "en-us" => Ok("en".to_string()),
        "ko" | "ko-kr" => Ok("ko".to_string()),
        _ => Err(format!("지원하지 않는 보드 언어입니다: {value}")),
    }
}

fn provider_accent_color(snapshot: Option<&UsageSnapshot>, provider_id: &str) -> Option<String> {
    snapshot?
        .providers
        .iter()
        .find(|provider| provider.id.eq_ignore_ascii_case(provider_id))
        .and_then(|provider| {
            provider
                .theme
                .as_ref()
                .map(|theme| theme.accent.clone())
                .or_else(|| provider.theme_color.clone())
        })
}

fn is_hex_color(value: &str) -> bool {
    let value = value.trim();
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    hex.len() == 6 && hex.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn image_mime(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        _ => "image/png",
    }
}

fn image_data_url(path: &std::path::Path) -> Result<String, String> {
    use base64::Engine;
    let bytes = std::fs::read(path).map_err(|err| format!("read {}: {err}", path.display()))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:{};base64,{}", image_mime(path), encoded))
}

fn first_error_line(label: &str, message: &str) -> String {
    match message.lines().next() {
        Some(first) if !first.trim().is_empty() => format!("{label}: {first}"),
        _ => label.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_starts_when_device_url_is_missing() {
        let settings = DesktopSettings::default();
        let setup = SetupFlow::new(&settings);

        assert!(setup.active);
        assert_eq!(setup.stage, SetupStage::WaitingForBoard);
    }

    #[test]
    fn dock_starts_when_device_url_is_saved() {
        let settings = DesktopSettings {
            device_url: "http://192.168.1.50".to_string(),
            ..Default::default()
        };
        let setup = SetupFlow::new(&settings);

        assert!(!setup.active);
        assert_eq!(setup.stage, SetupStage::Idle);
    }

    #[test]
    fn missing_supported_board_keeps_waiting() {
        let mut controller = DesktopController::new_for_test(DesktopSettings::default());

        controller.handle_board_detection(Ok(BoardDetectionReport {
            port: None,
            firmware_status: None,
        }));

        assert_eq!(controller.setup.stage, SetupStage::WaitingForBoard);
        assert!(controller.setup.next_scan.is_some());
        assert!(!controller.flash_running);
    }

    #[test]
    fn missing_firmware_requires_manual_flash() {
        let mut controller = DesktopController::new_for_test(DesktopSettings::default());

        controller.handle_board_detection(Ok(BoardDetectionReport {
            port: Some("/dev/ttyACM0".to_string()),
            firmware_status: None,
        }));

        assert_eq!(controller.setup.stage, SetupStage::NeedsFlash);
        assert!(!controller.flash_running);
    }

    #[test]
    fn flash_starts_only_after_confirmation() {
        let mut controller = DesktopController::new_for_test(DesktopSettings::default());
        controller.setup.stage = SetupStage::NeedsFlash;
        controller.setup.port = Some("/dev/ttyACM0".to_string());
        controller.settings.serial_port = "/dev/ttyACM0".to_string();

        controller
            .confirm_flash_firmware()
            .expect("confirmation should queue flash");

        assert_eq!(controller.setup.stage, SetupStage::Flashing);
        assert!(controller.flash_running);
    }
}

#[cfg(test)]
impl DesktopController {
    fn new_for_test(settings: DesktopSettings) -> Self {
        let mut controller = Self {
            setup: SetupFlow::new(&settings),
            settings,
            settings_path: std::env::temp_dir().join("quota-dock-desktop-test-settings.toml"),
            worker: Worker::new(),
            firmware: bundled_firmware(),
            status: None,
            latest_snapshot: None,
            available_providers: Vec::new(),
            sync_scheduler: SyncScheduler::default(),
            sync_enabled: false,
            device_language: "ko".to_string(),
            send_images_next_sync: true,
            pending_image_clears: BTreeSet::new(),
            log: Vec::new(),
            board_probe_running: false,
            flash_running: false,
            wifi_running: false,
            serial_status_running: false,
            http_status_running: false,
            sync_running: false,
            command_running: None,
            validating_images: BTreeSet::new(),
            last_sync_ok: None,
            last_sync_message: String::new(),
            save_error: None,
            queue_enabled: false,
        };
        controller.setup.next_scan = None;
        controller
    }
}

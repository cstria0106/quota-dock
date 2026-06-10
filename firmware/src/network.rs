use std::io::{self, BufRead, Write};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use embedded_svc::http::Method;
use embedded_svc::io::{Read, Write as EspWrite};
use embedded_svc::wifi::{AuthMethod, ClientConfiguration, Configuration};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::http::server::{Configuration as HttpConfiguration, EspHttpServer};
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs};
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};
use heapless::String as HeaplessString;
use serde::{Deserialize, Serialize};

const MAX_HTTP_BODY: usize = 64 * 1024;
const NETWORK_STACK_SIZE: usize = 24 * 1024;
const SERIAL_STACK_SIZE: usize = 24 * 1024;
const NVS_NAMESPACE: &str = "monitor";
const NVS_WIFI_SSID: &str = "wifi_ssid";
const NVS_WIFI_PASSWORD: &str = "wifi_pass";

pub type CommandReceiver = mpsc::Receiver<AppCommand>;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppCommand {
    Ping,
    SetBrightness { value: u8 },
    CycleUsageProvider,
    NetworkStatus { status: NetworkStatus },
    UpdateUsage { snapshot: UsageSnapshot },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NetworkStatus {
    pub has_credentials: bool,
    pub connected: bool,
    pub ip: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UsageSnapshot {
    pub providers: Vec<UsageProvider>,
    pub updated_at: String,
    #[serde(default)]
    pub updated_at_unix: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UsageProvider {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub theme_color: Option<String>,
    #[serde(default)]
    pub theme: Option<UsageTheme>,
    #[serde(default)]
    pub pixel_art: Option<UsagePixelArt>,
    pub source: String,
    pub account: Option<String>,
    pub plan: Option<String>,
    pub windows: Vec<UsageWindow>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UsageTheme {
    pub accent: String,
    pub panel: String,
    pub panel_soft: String,
    pub primary_panel: String,
    pub primary_panel_soft: String,
    pub track: String,
    pub pill: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UsagePixelArt {
    pub palette: Vec<String>,
    pub rows: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UsageWindow {
    pub kind: String,
    pub label: String,
    pub used_percent: u8,
    pub resets_at: Option<String>,
    #[serde(default)]
    pub resets_at_unix: Option<u64>,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WifiCredentials {
    pub ssid: String,
    pub password: String,
}

#[derive(Clone, Debug, Serialize)]
struct StatusResponse {
    mode: &'static str,
    connected: bool,
    ip: Option<String>,
    event: Option<String>,
    heap_free: u32,
    heap_internal_free: u32,
    heap_min_free: u32,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SerialRequest {
    Status,
    SetWifi { ssid: String, password: String },
    ClearWifi,
    Command { command: AppCommand },
}

#[derive(Clone, Debug, Serialize)]
struct ApiResponse {
    ok: bool,
    message: String,
}

#[derive(Clone, Debug)]
struct NetworkState {
    connected: bool,
    ip: Option<String>,
    event: Option<String>,
}

enum CredentialRequest {
    Set {
        credentials: WifiCredentials,
        response_tx: mpsc::Sender<Result<(), String>>,
    },
    Clear {
        response_tx: mpsc::Sender<Result<(), String>>,
    },
}

pub fn start() -> CommandReceiver {
    let (command_tx, command_rx) = mpsc::channel();
    thread::Builder::new()
        .stack_size(NETWORK_STACK_SIZE)
        .spawn(move || {
            if let Err(err) = network_task(command_tx) {
                println!("Network task failed: {err:?}");
            }
        })
        .expect("spawn network task");
    command_rx
}

fn network_task(command_tx: mpsc::Sender<AppCommand>) -> anyhow::Result<()> {
    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs_partition = EspDefaultNvsPartition::take()?;
    let (credential_tx, credential_rx) = mpsc::channel::<CredentialRequest>();
    let state = Arc::new(Mutex::new(NetworkState {
        connected: false,
        ip: None,
        event: Some("network_start".to_string()),
    }));

    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(
            peripherals.modem,
            sys_loop.clone(),
            Some(nvs_partition.clone()),
        )?,
        sys_loop,
    )?;
    set_wifi_storage_flash()?;
    start_serial_task(command_tx.clone(), credential_tx, state.clone());

    set_event(&state, "boot_load_wifi_config");
    let mut server: Option<EspHttpServer<'static>> = None;

    if let Some(credentials) = load_credentials(nvs_partition.clone())? {
        send_network_status(&command_tx, true, false, None);
        set_event(&state, "boot_connect_stored_credentials");
        if let Err(err) = connect_wifi(&mut wifi, Some(&credentials), state.clone()) {
            set_event(&state, "stored_wifi_connect_failed");
            send_network_status(&command_tx, true, false, None);
            println!("Stored Wi-Fi credentials failed: {err:?}");
        } else if server.is_none() {
            send_current_network_status(&command_tx, &state, true);
            set_event(&state, "http_server_start");
            server = Some(start_http_server(command_tx.clone(), state.clone())?);
            set_event(&state, "http_server_ready");
            send_current_network_status(&command_tx, &state, true);
        }
    } else {
        set_event(&state, "no_stored_credentials");
        send_network_status(&command_tx, false, false, None);
        println!("No stored Wi-Fi credentials. Send JSON over USB serial to provision.");
    }

    loop {
        if let Ok(request) = credential_rx.try_recv() {
            match request {
                CredentialRequest::Set {
                    credentials,
                    response_tx,
                } => {
                    set_event(&state, "save_wifi_config");
                    let save_result = save_credentials(nvs_partition.clone(), &credentials)
                        .map_err(|err| format!("{err:?}"));
                    let should_connect = save_result.is_ok();
                    let _ = response_tx.send(save_result);

                    if should_connect {
                        send_network_status(&command_tx, true, false, None);
                        set_event(&state, "connect_provisioned_credentials");
                        if let Err(err) = connect_wifi(&mut wifi, Some(&credentials), state.clone())
                        {
                            set_event(&state, "provisioned_wifi_connect_failed");
                            send_network_status(&command_tx, true, false, None);
                            println!("Provisioned Wi-Fi credentials failed: {err:?}");
                        } else if server.is_none() {
                            send_current_network_status(&command_tx, &state, true);
                            set_event(&state, "http_server_start");
                            server = Some(start_http_server(command_tx.clone(), state.clone())?);
                            set_event(&state, "http_server_ready");
                            send_current_network_status(&command_tx, &state, true);
                        } else {
                            send_current_network_status(&command_tx, &state, true);
                        }
                    }
                }
                CredentialRequest::Clear { response_tx } => {
                    set_event(&state, "clear_wifi_config");
                    let clear_result = clear_credentials(nvs_partition.clone())
                        .and_then(|_| stop_wifi(&mut wifi, state.clone()))
                        .map_err(|err| format!("{err:?}"));
                    let did_clear = clear_result.is_ok();
                    let _ = response_tx.send(clear_result);

                    if did_clear {
                        server = None;
                        send_network_status(&command_tx, false, false, None);
                        set_event(&state, "wifi_credentials_cleared");
                    }
                }
            }
        }

        let _ = &mut server;
        thread::sleep(Duration::from_secs(5));
    }
}

fn send_network_status(
    command_tx: &mpsc::Sender<AppCommand>,
    has_credentials: bool,
    connected: bool,
    ip: Option<String>,
) {
    let _ = command_tx.send(AppCommand::NetworkStatus {
        status: NetworkStatus {
            has_credentials,
            connected,
            ip,
        },
    });
}

fn send_current_network_status(
    command_tx: &mpsc::Sender<AppCommand>,
    state: &Arc<Mutex<NetworkState>>,
    has_credentials: bool,
) {
    let status = current_status(state);
    send_network_status(command_tx, has_credentials, status.connected, status.ip);
}

fn start_serial_task(
    command_tx: mpsc::Sender<AppCommand>,
    credential_tx: mpsc::Sender<CredentialRequest>,
    state: Arc<Mutex<NetworkState>>,
) {
    thread::Builder::new()
        .stack_size(SERIAL_STACK_SIZE)
        .spawn(move || {
            let stdin = io::stdin();
            let mut stdout = io::stdout();

            for line in stdin.lock().lines() {
                let response = match line {
                    Ok(line) if line.trim().is_empty() => continue,
                    Ok(line) => handle_serial_line(&line, &command_tx, &credential_tx, &state),
                    Err(err)
                        if err.kind() == io::ErrorKind::WouldBlock
                            || err.raw_os_error() == Some(11) =>
                    {
                        thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                    Err(err) => ApiResponse {
                        ok: false,
                        message: format!("serial read failed: {err}"),
                    },
                };

                match serde_json::to_string(&response) {
                    Ok(json) => {
                        let _ = writeln!(stdout, "{json}");
                        let _ = stdout.flush();
                    }
                    Err(err) => {
                        println!("{{\"ok\":false,\"message\":\"serialize failed: {err}\"}}")
                    }
                }
            }
        })
        .expect("spawn serial task");
}

fn handle_serial_line(
    line: &str,
    command_tx: &mpsc::Sender<AppCommand>,
    credential_tx: &mpsc::Sender<CredentialRequest>,
    state: &Arc<Mutex<NetworkState>>,
) -> ApiResponse {
    let request = match serde_json::from_str::<SerialRequest>(line.trim()) {
        Ok(request) => request,
        Err(err) => {
            return ApiResponse {
                ok: false,
                message: format!("invalid request: {err}"),
            }
        }
    };

    match request {
        SerialRequest::Status => ApiResponse {
            ok: true,
            message: serde_json::to_string(&current_status(state))
                .unwrap_or_else(|_| "status".to_string()),
        },
        SerialRequest::SetWifi { ssid, password } => {
            set_event(state, "serial_set_wifi_received");
            let (response_tx, response_rx) = mpsc::channel();
            if let Err(err) = credential_tx.send(CredentialRequest::Set {
                credentials: WifiCredentials { ssid, password },
                response_tx,
            }) {
                return ApiResponse {
                    ok: false,
                    message: format!("wifi credential queue failed: {err}"),
                };
            }
            set_event(state, "serial_set_wifi_queued");

            match response_rx.recv_timeout(Duration::from_secs(25)) {
                Ok(Ok(())) => ApiResponse {
                    ok: true,
                    message: "wifi credentials saved and queued".to_string(),
                },
                Ok(Err(err)) => ApiResponse {
                    ok: false,
                    message: format!("wifi credential save failed: {err}"),
                },
                Err(err) => {
                    set_event(state, "serial_set_wifi_response_timeout");
                    ApiResponse {
                        ok: false,
                        message: format!("wifi credential save timed out: {err}"),
                    }
                }
            }
        }
        SerialRequest::ClearWifi => {
            set_event(state, "serial_clear_wifi_received");
            let (response_tx, response_rx) = mpsc::channel();
            if let Err(err) = credential_tx.send(CredentialRequest::Clear { response_tx }) {
                return ApiResponse {
                    ok: false,
                    message: format!("wifi credential clear queue failed: {err}"),
                };
            }
            set_event(state, "serial_clear_wifi_queued");

            match response_rx.recv_timeout(Duration::from_secs(25)) {
                Ok(Ok(())) => ApiResponse {
                    ok: true,
                    message: "wifi credentials cleared".to_string(),
                },
                Ok(Err(err)) => ApiResponse {
                    ok: false,
                    message: format!("wifi credential clear failed: {err}"),
                },
                Err(err) => {
                    set_event(state, "serial_clear_wifi_response_timeout");
                    ApiResponse {
                        ok: false,
                        message: format!("wifi credential clear timed out: {err}"),
                    }
                }
            }
        }
        SerialRequest::Command { command } => match command_tx.send(command) {
            Ok(()) => ApiResponse {
                ok: true,
                message: "command queued".to_string(),
            },
            Err(err) => ApiResponse {
                ok: false,
                message: format!("command failed: {err}"),
            },
        },
    }
}

fn start_http_server(
    command_tx: mpsc::Sender<AppCommand>,
    state: Arc<Mutex<NetworkState>>,
) -> anyhow::Result<EspHttpServer<'static>> {
    let mut server = EspHttpServer::new(&HttpConfiguration {
        stack_size: 10240,
        ..Default::default()
    })?;

    let status_state = state.clone();
    server.fn_handler("/status", Method::Get, move |req| {
        let body = serde_json::to_vec(&current_status(&status_state))?;
        req.into_ok_response()?.write_all(&body)?;
        Ok::<(), anyhow::Error>(())
    })?;

    let command_tx_for_command = command_tx.clone();
    server.fn_handler("/command", Method::Post, move |mut req| {
        let body = read_body(&mut req)?;
        let command = serde_json::from_slice::<AppCommand>(&body)?;
        command_tx_for_command.send(command)?;
        let response = serde_json::to_vec(&ApiResponse {
            ok: true,
            message: "command queued".to_string(),
        })?;
        req.into_ok_response()?.write_all(&response)?;
        Ok::<(), anyhow::Error>(())
    })?;

    server.fn_handler("/usage", Method::Post, move |mut req| {
        let body = read_body(&mut req)?;
        let snapshot = serde_json::from_slice::<UsageSnapshot>(&body)?;
        command_tx.send(AppCommand::UpdateUsage { snapshot })?;
        let response = serde_json::to_vec(&ApiResponse {
            ok: true,
            message: "usage snapshot queued".to_string(),
        })?;
        req.into_ok_response()?.write_all(&response)?;
        Ok::<(), anyhow::Error>(())
    })?;

    Ok(server)
}

fn read_body<T>(req: &mut T) -> anyhow::Result<Vec<u8>>
where
    T: Read,
{
    let mut body = Vec::new();
    let mut buffer = [0_u8; 64];

    loop {
        let len = req
            .read(&mut buffer)
            .map_err(|_| anyhow::anyhow!("request read failed"))?;
        if len == 0 {
            break;
        }
        body.extend_from_slice(&buffer[..len]);
        if body.len() > MAX_HTTP_BODY {
            anyhow::bail!("request body too large");
        }
    }

    Ok(body)
}

fn set_wifi_mode_sta() -> anyhow::Result<()> {
    esp_idf_sys::esp!(unsafe {
        esp_idf_sys::esp_wifi_set_mode(esp_idf_sys::wifi_mode_t_WIFI_MODE_STA)
    })?;
    Ok(())
}

fn set_wifi_storage_flash() -> anyhow::Result<()> {
    esp_idf_sys::esp!(unsafe {
        esp_idf_sys::esp_wifi_set_storage(esp_idf_sys::wifi_storage_t_WIFI_STORAGE_FLASH)
    })?;
    Ok(())
}

fn set_wifi_configuration(
    wifi: &mut BlockingWifi<EspWifi<'static>>,
    credentials: &WifiCredentials,
) -> anyhow::Result<()> {
    if wifi.is_started().unwrap_or(false) {
        let _ = wifi.disconnect();
        wifi.stop()?;
    }

    wifi.set_configuration(&Configuration::Client(client_configuration(credentials)?))?;
    Ok(())
}

fn connect_wifi(
    wifi: &mut BlockingWifi<EspWifi<'static>>,
    credentials: Option<&WifiCredentials>,
    state: Arc<Mutex<NetworkState>>,
) -> anyhow::Result<()> {
    if let Some(credentials) = credentials {
        set_wifi_configuration(wifi, credentials)?;
    } else if wifi.is_started().unwrap_or(false) {
        let _ = wifi.disconnect();
        wifi.stop()?;
        set_wifi_mode_sta()?;
    } else {
        set_wifi_mode_sta()?;
    }

    wifi.start()?;
    wifi.connect()?;
    wifi.wait_netif_up()?;

    let ip = wifi.wifi().sta_netif().get_ip_info()?.ip.to_string();
    if let Ok(mut state) = state.lock() {
        state.connected = true;
        state.ip = Some(ip.clone());
        state.event = Some("wifi_connected".to_string());
    }
    println!("Wi-Fi connected at {ip}");

    Ok(())
}

fn stop_wifi(
    wifi: &mut BlockingWifi<EspWifi<'static>>,
    state: Arc<Mutex<NetworkState>>,
) -> anyhow::Result<()> {
    if wifi.is_started().unwrap_or(false) {
        let _ = wifi.disconnect();
        wifi.stop()?;
    }
    set_wifi_mode_sta()?;
    if let Ok(mut state) = state.lock() {
        state.connected = false;
        state.ip = None;
        state.event = Some("wifi_stopped".to_string());
    }
    Ok(())
}

fn client_configuration(credentials: &WifiCredentials) -> anyhow::Result<ClientConfiguration> {
    let ssid: HeaplessString<32> = credentials
        .ssid
        .as_str()
        .try_into()
        .map_err(|_| anyhow::anyhow!("ssid must be 32 bytes or shorter"))?;
    let password: HeaplessString<64> = credentials
        .password
        .as_str()
        .try_into()
        .map_err(|_| anyhow::anyhow!("password must be 64 bytes or shorter"))?;

    Ok(ClientConfiguration {
        ssid,
        password,
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    })
}

fn current_status(state: &Arc<Mutex<NetworkState>>) -> StatusResponse {
    let state = state.lock().ok();
    StatusResponse {
        mode: "wifi_sta",
        connected: state.as_ref().map(|state| state.connected).unwrap_or(false),
        ip: state.as_ref().and_then(|state| state.ip.clone()),
        event: state.and_then(|state| state.event.clone()),
        heap_free: unsafe { esp_idf_sys::esp_get_free_heap_size() },
        heap_internal_free: unsafe { esp_idf_sys::esp_get_free_internal_heap_size() },
        heap_min_free: unsafe { esp_idf_sys::esp_get_minimum_free_heap_size() },
    }
}

fn load_credentials(partition: EspDefaultNvsPartition) -> anyhow::Result<Option<WifiCredentials>> {
    let nvs = EspNvs::new(partition, NVS_NAMESPACE, true)?;
    let mut ssid_buffer = [0_u8; 64];
    let mut password_buffer = [0_u8; 96];
    let ssid = nvs.get_str(NVS_WIFI_SSID, &mut ssid_buffer)?;
    let password = nvs.get_str(NVS_WIFI_PASSWORD, &mut password_buffer)?;

    Ok(match (ssid, password) {
        (Some(ssid), Some(password)) if !ssid.is_empty() => Some(WifiCredentials {
            ssid: ssid.to_string(),
            password: password.to_string(),
        }),
        _ => None,
    })
}

fn save_credentials(
    partition: EspDefaultNvsPartition,
    credentials: &WifiCredentials,
) -> anyhow::Result<()> {
    let nvs = EspNvs::new(partition.clone(), NVS_NAMESPACE, true)?;
    nvs.set_str(NVS_WIFI_SSID, &credentials.ssid)?;
    nvs.set_str(NVS_WIFI_PASSWORD, &credentials.password)?;

    match load_credentials(partition)? {
        Some(saved) if saved.ssid == credentials.ssid && saved.password == credentials.password => {
            Ok(())
        }
        _ => anyhow::bail!("saved Wi-Fi credentials did not round-trip through NVS"),
    }
}

fn clear_credentials(partition: EspDefaultNvsPartition) -> anyhow::Result<()> {
    let nvs = EspNvs::new(partition.clone(), NVS_NAMESPACE, true)?;
    let _ = nvs.remove(NVS_WIFI_SSID)?;
    let _ = nvs.remove(NVS_WIFI_PASSWORD)?;

    if load_credentials(partition)?.is_some() {
        anyhow::bail!("cleared Wi-Fi credentials still exist in NVS");
    }
    Ok(())
}

fn set_event(state: &Arc<Mutex<NetworkState>>, event: &str) {
    if let Ok(mut state) = state.lock() {
        state.event = Some(event.to_string());
    }
}

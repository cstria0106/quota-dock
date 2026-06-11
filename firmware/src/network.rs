use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use embedded_svc::wifi::{AuthMethod, ClientConfiguration, Configuration};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs};
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};
use heapless::String as HeaplessString;
use serde::{Deserialize, Serialize};

const MAX_FRAME_BODY: usize = 64 * 1024;
const SERIAL_FRAME_PREFIX: &[u8] = b"QD1:";
const MAX_ENCODED_FRAME: usize = (MAX_FRAME_BODY + 3) * 2 + SERIAL_FRAME_PREFIX.len() + 2;
const PROTOCOL_PORT: u16 = 3333;
const SERIAL_FRAME_VERSION: u8 = 1;
const COMMAND_QUEUE_CAPACITY: usize = 8;
const NETWORK_STACK_SIZE: usize = 24 * 1024;
const PROTOCOL_STACK_SIZE: usize = 18 * 1024;
const SERIAL_STACK_SIZE: usize = 24 * 1024;
const SERIAL_IDLE_DELAY_MS: u32 = 5;
const SERIAL_READ_BUFFER_BYTES: usize = 256;
const NVS_NAMESPACE: &str = "quota-dock";
const NVS_WIFI_SSID: &str = "wifi_ssid";
const NVS_WIFI_PASSWORD: &str = "wifi_pass";

pub type CommandReceiver = mpsc::Receiver<AppCommand>;
pub type ProviderImageStatuses = Arc<Mutex<Vec<ProviderImageStatus>>>;
type CommandSender = mpsc::SyncSender<AppCommand>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum AppCommand {
    Ping,
    SetBrightness { value: u8 },
    CycleUsageProvider,
    NetworkStatus { status: NetworkStatus },
    UpdateUsage { snapshot: UsageSnapshot },
    UpdateUsageProvider { update: UsageProviderUpdate },
    Sync { payload: SyncPayload },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
enum DeviceCommand {
    Ping,
    SetBrightness { value: u8 },
    CycleUsageProvider,
}

impl From<DeviceCommand> for AppCommand {
    fn from(command: DeviceCommand) -> Self {
        match command {
            DeviceCommand::Ping => Self::Ping,
            DeviceCommand::SetBrightness { value } => Self::SetBrightness { value },
            DeviceCommand::CycleUsageProvider => Self::CycleUsageProvider,
        }
    }
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
pub struct UsageProviderUpdate {
    pub provider: UsageProvider,
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
pub struct SyncPayload {
    pub visible_provider_ids: Vec<String>,
    pub providers: Vec<ProviderSync>,
    pub updated_at: String,
    #[serde(default)]
    pub updated_at_unix: u64,
    #[serde(default)]
    pub language: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProviderSync {
    pub id: String,
    pub usage: Option<UsageProvider>,
    pub image_id: Option<u32>,
    pub pixel_art: Option<UsagePixelArt>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SyncResponse {
    pub ok: bool,
    pub missing_images: Vec<String>,
    pub message: String,
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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProviderImageStatus {
    pub provider_id: String,
    pub image_id: u32,
}

#[derive(Clone, Debug, Serialize)]
struct StatusResponse {
    mode: &'static str,
    connected: bool,
    ip: Option<String>,
    event: Option<String>,
    firmware_version: Option<&'static str>,
    firmware_hash: Option<&'static str>,
    heap_free: u32,
    heap_internal_free: u32,
    heap_min_free: u32,
}

#[derive(Debug, Deserialize)]
enum SerialRequest {
    Status,
    SetWifi { ssid: String, password: String },
    ClearWifi,
    Command(DeviceCommand),
}

#[derive(Clone, Debug, Serialize)]
struct ApiResponse {
    ok: bool,
    message: String,
}

#[derive(Clone, Debug, Serialize)]
enum SerialReply {
    Status(StatusResponse),
    Api(ApiResponse),
}

#[derive(Clone, Debug)]
struct NetworkState {
    connected: bool,
    ip: Option<String>,
    event: Option<String>,
}

enum CredentialRequest {
    Set { credentials: WifiCredentials },
    Clear,
}

pub fn start(provider_images: ProviderImageStatuses) -> CommandReceiver {
    let (command_tx, command_rx) = mpsc::sync_channel(COMMAND_QUEUE_CAPACITY);
    thread::Builder::new()
        .stack_size(NETWORK_STACK_SIZE)
        .spawn(move || {
            if let Err(err) = network_task(command_tx, provider_images) {
                println!("Network task failed: {err:?}");
            }
        })
        .expect("spawn network task");
    command_rx
}

fn network_task(
    command_tx: CommandSender,
    provider_images: ProviderImageStatuses,
) -> anyhow::Result<()> {
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
    start_serial_task(
        command_tx.clone(),
        credential_tx,
        state.clone(),
        nvs_partition.clone(),
    );

    set_event(&state, "boot_load_wifi_config");
    let mut protocol_started = false;

    if let Some(credentials) = load_credentials(nvs_partition.clone())? {
        send_network_status(&command_tx, true, false, None);
        set_event(&state, "boot_connect_stored_credentials");
        if let Err(err) = connect_wifi(&mut wifi, Some(&credentials), state.clone()) {
            set_event(&state, "stored_wifi_connect_failed");
            send_network_status(&command_tx, true, false, None);
            println!("Stored Wi-Fi credentials failed: {err:?}");
        } else if !protocol_started {
            send_current_network_status(&command_tx, &state, true);
            set_event(&state, "protocol_server_start");
            start_protocol_server(command_tx.clone(), state.clone(), provider_images.clone())?;
            protocol_started = true;
            set_event(&state, "protocol_server_ready");
            send_current_network_status(&command_tx, &state, true);
        }
    } else {
        set_event(&state, "no_stored_credentials");
        send_network_status(&command_tx, false, false, None);
        println!("No stored Wi-Fi credentials. Send JSON over USB serial to provision.");
    }

    loop {
        let request = match credential_rx.recv() {
            Ok(request) => request,
            Err(_) => return Ok(()),
        };

        match request {
            CredentialRequest::Set { credentials } => {
                send_network_status(&command_tx, true, false, None);
                set_event(&state, "connect_provisioned_credentials");
                if let Err(err) = connect_wifi(&mut wifi, Some(&credentials), state.clone()) {
                    set_event(&state, "provisioned_wifi_connect_failed");
                    send_network_status(&command_tx, true, false, None);
                    println!("Provisioned Wi-Fi credentials failed: {err:?}");
                } else if !protocol_started {
                    send_current_network_status(&command_tx, &state, true);
                    set_event(&state, "protocol_server_start");
                    start_protocol_server(
                        command_tx.clone(),
                        state.clone(),
                        provider_images.clone(),
                    )?;
                    protocol_started = true;
                    set_event(&state, "protocol_server_ready");
                    send_current_network_status(&command_tx, &state, true);
                } else {
                    send_current_network_status(&command_tx, &state, true);
                }
            }
            CredentialRequest::Clear => {
                set_event(&state, "wifi_credentials_cleared");
                match stop_wifi(&mut wifi, state.clone()) {
                    Ok(()) => send_network_status(&command_tx, false, false, None),
                    Err(err) => {
                        set_event(&state, "wifi_stop_after_clear_failed");
                        send_current_network_status(&command_tx, &state, false);
                        println!("Wi-Fi stop after clear failed: {err:?}");
                    }
                }
            }
        }
    }
}

fn send_network_status(
    command_tx: &CommandSender,
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
    command_tx: &CommandSender,
    state: &Arc<Mutex<NetworkState>>,
    has_credentials: bool,
) {
    let state = state.lock().ok();
    send_network_status(
        command_tx,
        has_credentials,
        state.as_ref().map(|state| state.connected).unwrap_or(false),
        state.as_ref().and_then(|state| state.ip.clone()),
    );
}

fn start_serial_task(
    command_tx: CommandSender,
    credential_tx: mpsc::Sender<CredentialRequest>,
    state: Arc<Mutex<NetworkState>>,
    nvs_partition: EspDefaultNvsPartition,
) {
    thread::Builder::new()
        .stack_size(SERIAL_STACK_SIZE)
        .spawn(move || {
            let mut stdin = io::stdin();
            let stdout = io::stdout();
            let mut input = Vec::new();
            let mut buffer = [0_u8; SERIAL_READ_BUFFER_BYTES];

            loop {
                match stdin.read(&mut buffer) {
                    Ok(0) => {
                        FreeRtos::delay_ms(SERIAL_IDLE_DELAY_MS);
                    }
                    Ok(len) => {
                        input.extend_from_slice(&buffer[..len]);
                        while let Some(frame) = try_take_serial_frame(&mut input) {
                            let reply = match postcard::from_bytes::<SerialRequest>(&frame) {
                                Ok(request) => handle_serial_request(
                                    request,
                                    &command_tx,
                                    &credential_tx,
                                    &state,
                                    &nvs_partition,
                                ),
                                Err(err) => {
                                    println!("Invalid serial request: {err}");
                                    continue;
                                }
                            };

                            match postcard::to_allocvec(&reply) {
                                Ok(body) => {
                                    let mut stdout = stdout.lock();
                                    let _ = write_serial_frame(&mut stdout, &body);
                                }
                                Err(err) => println!("Serial reply encode failed: {err}"),
                            }
                        }
                    }
                    Err(err)
                        if err.kind() == io::ErrorKind::WouldBlock
                            || err.raw_os_error() == Some(11) =>
                    {
                        FreeRtos::delay_ms(SERIAL_IDLE_DELAY_MS);
                    }
                    Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                    Err(err) => println!("Serial read failed: {err}"),
                }
            }
        })
        .expect("spawn serial task");
}

fn handle_serial_request(
    request: SerialRequest,
    command_tx: &CommandSender,
    credential_tx: &mpsc::Sender<CredentialRequest>,
    state: &Arc<Mutex<NetworkState>>,
    nvs_partition: &EspDefaultNvsPartition,
) -> SerialReply {
    match request {
        SerialRequest::Status => SerialReply::Status(current_status(state)),
        SerialRequest::SetWifi { ssid, password } => {
            set_event(state, "serial_set_wifi_received");
            let credentials = WifiCredentials { ssid, password };
            let save_result = save_credentials(nvs_partition.clone(), &credentials)
                .map_err(|err| format!("{err:?}"));
            if let Err(err) = save_result {
                return SerialReply::Api(ApiResponse {
                    ok: false,
                    message: format!("wifi credential save failed: {err}"),
                });
            }
            if let Err(err) = credential_tx.send(CredentialRequest::Set { credentials }) {
                println!("Wi-Fi connect queue after save failed: {err}");
            }
            set_event(state, "serial_set_wifi_queued");

            SerialReply::Api(ApiResponse {
                ok: true,
                message: "wifi credentials saved".to_string(),
            })
        }
        SerialRequest::ClearWifi => {
            set_event(state, "serial_clear_wifi_received");
            let clear_result =
                clear_credentials(nvs_partition.clone()).map_err(|err| format!("{err:?}"));
            if let Err(err) = clear_result {
                return SerialReply::Api(ApiResponse {
                    ok: false,
                    message: format!("wifi credential clear failed: {err}"),
                });
            }
            if let Err(err) = credential_tx.send(CredentialRequest::Clear) {
                println!("Wi-Fi stop queue after clear failed: {err}");
            }
            set_event(state, "serial_clear_wifi_queued");

            SerialReply::Api(ApiResponse {
                ok: true,
                message: "wifi credentials cleared".to_string(),
            })
        }
        SerialRequest::Command(command) => {
            SerialReply::Api(match command_tx.send(command.into()) {
                Ok(()) => ApiResponse {
                    ok: true,
                    message: "command queued".to_string(),
                },
                Err(err) => ApiResponse {
                    ok: false,
                    message: format!("command failed: {err}"),
                },
            })
        }
    }
}

fn try_take_serial_frame(input: &mut Vec<u8>) -> Option<Vec<u8>> {
    loop {
        if input.len() > MAX_ENCODED_FRAME {
            let drain_to = input
                .iter()
                .position(is_line_ending)
                .map(|position| position + 1)
                .unwrap_or(input.len());
            input.drain(..drain_to);
        }

        let end = input.iter().position(is_line_ending)?;
        let mut line = input.drain(..=end).collect::<Vec<_>>();
        while line.last().is_some_and(is_line_ending) {
            line.pop();
        }
        if line.is_empty() {
            continue;
        }

        let Some(prefix_start) = find_bytes(&line, SERIAL_FRAME_PREFIX) else {
            continue;
        };
        let Some(frame) = decode_hex_ascii(&line[prefix_start + SERIAL_FRAME_PREFIX.len()..])
        else {
            continue;
        };
        if frame.len() < 3 || frame[0] != SERIAL_FRAME_VERSION {
            continue;
        }
        let crc = u16::from_le_bytes([frame[1], frame[2]]);
        let body = frame[3..].to_vec();
        if crc != crc16_ccitt(&body) {
            continue;
        }
        return Some(body);
    }
}

fn write_serial_frame(output: &mut impl Write, body: &[u8]) -> anyhow::Result<()> {
    if body.len() > MAX_FRAME_BODY {
        anyhow::bail!("serial frame too large: {} bytes", body.len());
    }
    let crc = crc16_ccitt(body).to_le_bytes();
    let mut frame = Vec::with_capacity(SERIAL_FRAME_PREFIX.len() + (body.len() + 3) * 2 + 1);
    frame.extend_from_slice(SERIAL_FRAME_PREFIX);
    push_hex_byte(&mut frame, SERIAL_FRAME_VERSION);
    push_hex_byte(&mut frame, crc[0]);
    push_hex_byte(&mut frame, crc[1]);
    for byte in body {
        push_hex_byte(&mut frame, *byte);
    }
    frame.push(b'\n');
    output.write_all(&frame)?;
    output.flush()?;
    Ok(())
}

fn is_line_ending(byte: &u8) -> bool {
    matches!(*byte, b'\n' | b'\r')
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn push_hex_byte(output: &mut Vec<u8>, byte: u8) {
    output.push(hex_digit(byte >> 4));
    output.push(hex_digit(byte & 0x0f));
}

fn hex_digit(nibble: u8) -> u8 {
    match nibble {
        0..=9 => b'0' + nibble,
        10..=15 => b'a' + (nibble - 10),
        _ => unreachable!("nibble is masked to four bits"),
    }
}

fn decode_hex_ascii(input: &[u8]) -> Option<Vec<u8>> {
    let chunks = input.chunks_exact(2);
    if !chunks.remainder().is_empty() {
        return None;
    }
    let mut output = Vec::with_capacity(input.len() / 2);
    for chunk in chunks {
        let high = hex_value(chunk[0])?;
        let low = hex_value(chunk[1])?;
        output.push((high << 4) | low);
    }
    Some(output)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn crc16_ccitt(bytes: &[u8]) -> u16 {
    let mut crc = 0xffff_u16;
    for byte in bytes {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            if crc & 0x8000 == 0 {
                crc <<= 1;
            } else {
                crc = (crc << 1) ^ 0x1021;
            }
        }
    }
    crc
}

fn start_protocol_server(
    command_tx: CommandSender,
    state: Arc<Mutex<NetworkState>>,
    provider_images: ProviderImageStatuses,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", PROTOCOL_PORT))?;
    println!("Device protocol listening on tcp/{PROTOCOL_PORT}");
    thread::Builder::new()
        .stack_size(PROTOCOL_STACK_SIZE)
        .spawn(move || protocol_task(listener, command_tx, state, provider_images))
        .expect("spawn protocol task");
    Ok(())
}

fn protocol_task(
    listener: TcpListener,
    command_tx: CommandSender,
    state: Arc<Mutex<NetworkState>>,
    provider_images: ProviderImageStatuses,
) {
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(err) =
                    handle_protocol_client(&mut stream, &command_tx, &state, &provider_images)
                {
                    println!("Protocol request failed: {err:?}");
                }
            }
            Err(err) => println!("Protocol accept failed: {err}"),
        }
    }
}

fn handle_protocol_client(
    stream: &mut TcpStream,
    command_tx: &CommandSender,
    state: &Arc<Mutex<NetworkState>>,
    provider_images: &ProviderImageStatuses,
) -> anyhow::Result<()> {
    let request_body = read_frame(stream)?;
    let request = postcard::from_bytes::<DeviceRequest>(&request_body)?;
    let reply = handle_device_request(request, command_tx, state, provider_images);
    let reply_body = postcard::to_allocvec(&reply)?;
    write_frame(stream, &reply_body)
}

fn handle_device_request(
    request: DeviceRequest,
    command_tx: &CommandSender,
    state: &Arc<Mutex<NetworkState>>,
    provider_images: &ProviderImageStatuses,
) -> DeviceReply {
    match request {
        DeviceRequest::Status => DeviceReply::Status(current_status(state)),
        DeviceRequest::Command(command) => match command_tx.send(command.into()) {
            Ok(()) => DeviceReply::Api(ApiResponse {
                ok: true,
                message: "command queued".to_string(),
            }),
            Err(err) => DeviceReply::Api(ApiResponse {
                ok: false,
                message: format!("command failed: {err}"),
            }),
        },
        DeviceRequest::Usage(snapshot) => {
            match command_tx.send(AppCommand::UpdateUsage { snapshot }) {
                Ok(()) => DeviceReply::Api(ApiResponse {
                    ok: true,
                    message: "usage snapshot queued".to_string(),
                }),
                Err(err) => DeviceReply::Api(ApiResponse {
                    ok: false,
                    message: format!("usage snapshot failed: {err}"),
                }),
            }
        }
        DeviceRequest::UsageProvider(update) => {
            match command_tx.send(AppCommand::UpdateUsageProvider { update }) {
                Ok(()) => DeviceReply::Api(ApiResponse {
                    ok: true,
                    message: "usage provider queued".to_string(),
                }),
                Err(err) => DeviceReply::Api(ApiResponse {
                    ok: false,
                    message: format!("usage provider failed: {err}"),
                }),
            }
        }
        DeviceRequest::Sync(payload) => {
            let missing_images = missing_images(&payload, provider_images);
            match command_tx.send(AppCommand::Sync { payload }) {
                Ok(()) => DeviceReply::Sync(SyncResponse {
                    ok: true,
                    missing_images,
                    message: "sync queued".to_string(),
                }),
                Err(err) => DeviceReply::Sync(SyncResponse {
                    ok: false,
                    missing_images: Vec::new(),
                    message: format!("sync failed: {err}"),
                }),
            }
        }
    }
}

fn missing_images(payload: &SyncPayload, provider_images: &ProviderImageStatuses) -> Vec<String> {
    let cached_images = provider_images
        .lock()
        .map(|images| images.clone())
        .unwrap_or_default();
    payload
        .providers
        .iter()
        .filter(|provider| provider.pixel_art.is_none())
        .filter_map(|provider| {
            let image_id = provider.image_id?;
            let has_image = cached_images.iter().any(|cached| {
                cached
                    .provider_id
                    .eq_ignore_ascii_case(provider.id.as_str())
                    && cached.image_id == image_id
            });
            (!has_image).then(|| provider.id.clone())
        })
        .collect()
}

fn read_frame(stream: &mut TcpStream) -> anyhow::Result<Vec<u8>> {
    let mut len_bytes = [0_u8; 4];
    stream.read_exact(&mut len_bytes)?;
    let len = u32::from_le_bytes(len_bytes) as usize;
    if len > MAX_FRAME_BODY {
        anyhow::bail!("frame too large: {len} bytes");
    }

    let mut body = vec![0; len];
    stream.read_exact(&mut body)?;
    Ok(body)
}

fn write_frame(stream: &mut TcpStream, body: &[u8]) -> anyhow::Result<()> {
    if body.len() > MAX_FRAME_BODY {
        anyhow::bail!("frame too large: {} bytes", body.len());
    }
    stream.write_all(&(body.len() as u32).to_le_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

#[derive(Debug, Deserialize)]
enum DeviceRequest {
    Status,
    Command(DeviceCommand),
    Usage(UsageSnapshot),
    UsageProvider(UsageProviderUpdate),
    Sync(SyncPayload),
}

#[derive(Debug, Serialize)]
enum DeviceReply {
    Status(StatusResponse),
    Api(ApiResponse),
    Sync(SyncResponse),
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
        firmware_version: Some(env!("CARGO_PKG_VERSION")),
        firmware_hash: Some(env!("QUOTA_DOCK_FIRMWARE_HASH")),
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

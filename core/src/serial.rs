use std::fmt;
use std::io::{self, Read, Write};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serialport::ClearBuffer;

use crate::{ApiResponse, DeviceCommand, StatusResponse};

const SERIAL_FRAME_VERSION: u8 = 1;
const SERIAL_FRAME_PREFIX: &[u8] = b"QD1:";
const SERIAL_MAX_FRAME_BODY: usize = 64 * 1024;
const SERIAL_MAX_ENCODED_FRAME: usize =
    (SERIAL_MAX_FRAME_BODY + 3) * 2 + SERIAL_FRAME_PREFIX.len() + 2;
const SERIAL_OPEN_DELAY_MS: u64 = 100;
const SERIAL_RETRY_DELAY_MS: u64 = 200;
const SERIAL_READ_TIMEOUT_MS: u64 = 25;
const SERIAL_IDLE_SLEEP_MS: u64 = 5;
const SERIAL_READ_BUFFER_BYTES: usize = 256;
const SERIAL_SAMPLE_BYTES: usize = 32;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SerialRequest {
    Status,
    SetWifi { ssid: String, password: String },
    ClearWifi,
    Command(DeviceCommand),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SerialReply {
    Status(StatusResponse),
    Api(ApiResponse),
}

pub fn serial_port_names() -> Result<Vec<String>, String> {
    serialport::available_ports()
        .map_err(|err| err.to_string())
        .map(|ports| ports.into_iter().map(|port| port.port_name).collect())
}

pub fn send_serial(
    port_name: &str,
    baud: u32,
    request: &SerialRequest,
    timeout: Duration,
) -> Result<ApiResponse, String> {
    let reply =
        send_serial_reply(port_name, baud, request, timeout).map_err(|err| err.to_string())?;
    match reply {
        SerialReply::Api(response) => Ok(response),
        SerialReply::Status(_) => Err("expected api serial reply, got status".to_string()),
    }
}

pub fn send_serial_status(
    port_name: &str,
    baud: u32,
    timeout: Duration,
) -> Result<StatusResponse, String> {
    let reply = send_serial_reply(port_name, baud, &SerialRequest::Status, timeout)
        .map_err(|err| err.to_string())?;
    match reply {
        SerialReply::Status(status) => Ok(status),
        SerialReply::Api(response) => Err(format!(
            "expected status serial reply: {}",
            response.message
        )),
    }
}

fn send_serial_reply(
    port_name: &str,
    baud: u32,
    request: &SerialRequest,
    timeout: Duration,
) -> Result<SerialReply, Box<SerialFailure>> {
    let mut diagnostics = SerialDiagnostics::new(port_name, baud, request, timeout);
    match send_serial_once(port_name, baud, request, timeout, &mut diagnostics, 1) {
        Ok(response) => Ok(response),
        Err(err) if err.is_retryable_for(request) => {
            thread::sleep(Duration::from_millis(SERIAL_RETRY_DELAY_MS));
            match send_serial_once(port_name, baud, request, timeout, &mut diagnostics, 2) {
                Ok(response) => Ok(response),
                Err(err) => Err(Box::new(SerialFailure { err, diagnostics })),
            }
        }
        Err(err) => Err(Box::new(SerialFailure { err, diagnostics })),
    }
}

fn send_serial_once(
    port_name: &str,
    baud: u32,
    request: &SerialRequest,
    timeout: Duration,
    diagnostics: &mut SerialDiagnostics,
    attempt_index: usize,
) -> Result<SerialReply, SerialError> {
    let mut attempt = SerialAttemptDiagnostics::new(attempt_index);
    let attempt_started = Instant::now();
    let result = (|| {
        configure_serial_terminal(port_name, baud)?;
        attempt.terminal_configured = true;
        let mut port = serialport::new(port_name, baud)
            .timeout(Duration::from_millis(SERIAL_READ_TIMEOUT_MS))
            .dtr_on_open(false)
            .open()
            .map_err(|err| SerialError::port("open serial", err))?;
        attempt.port_opened = true;
        port.write_data_terminal_ready(false)
            .map_err(|err| SerialError::port("set serial DTR", err))?;
        attempt.dtr_low = true;
        port.write_request_to_send(false)
            .map_err(|err| SerialError::port("set serial RTS", err))?;
        attempt.rts_low = true;
        thread::sleep(Duration::from_millis(SERIAL_OPEN_DELAY_MS));
        let _ = port.clear(ClearBuffer::Input);
        let request = postcard::to_allocvec(request)
            .map_err(|err| SerialError::protocol(format!("encode serial request: {err}")))?;
        attempt.body_bytes = request.len();
        attempt.frame_bytes = write_serial_frame(&mut port, &request)?;

        let started = Instant::now();
        let mut input = Vec::new();
        let mut buffer = [0_u8; SERIAL_READ_BUFFER_BYTES];
        while started.elapsed() < timeout {
            match port.read(&mut buffer) {
                Ok(0) => thread::sleep(Duration::from_millis(SERIAL_IDLE_SLEEP_MS)),
                Ok(len) => {
                    attempt.record_read(&buffer[..len]);
                    input.extend_from_slice(&buffer[..len]);
                    attempt.pending_bytes = input.len();
                    if let Some(reply) = try_read_serial_reply(&mut input, &mut attempt) {
                        attempt.pending_bytes = input.len();
                        return Ok(reply);
                    }
                    attempt.pending_bytes = input.len();
                }
                Err(err) if err.kind() == std::io::ErrorKind::TimedOut => {
                    attempt.read_timeouts += 1;
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
                Err(err) => return Err(SerialError::io("serial read", err)),
            }
        }

        Err(SerialError::Timeout)
    })();

    attempt.elapsed_ms = attempt_started.elapsed().as_millis();
    match result {
        Ok(reply) => {
            diagnostics.attempts.push(attempt);
            Ok(reply)
        }
        Err(err) => {
            attempt.error = Some(err.to_string());
            diagnostics.attempts.push(attempt);
            Err(err)
        }
    }
}

fn write_serial_frame(port: &mut dyn Write, body: &[u8]) -> Result<usize, SerialError> {
    if body.len() > SERIAL_MAX_FRAME_BODY {
        return Err(SerialError::protocol(format!(
            "serial frame too large: {} bytes",
            body.len()
        )));
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
    port.write_all(&frame)
        .map_err(|err| SerialError::io("serial write", err))?;
    Ok(frame.len())
}

fn try_read_serial_reply(
    input: &mut Vec<u8>,
    diagnostics: &mut SerialAttemptDiagnostics,
) -> Option<SerialReply> {
    loop {
        let body = try_take_serial_frame(input, diagnostics)?;
        match postcard::from_bytes(&body) {
            Ok(reply) => return Some(reply),
            Err(_) => {
                diagnostics.reply_decode_errors += 1;
            }
        }
    }
}

fn try_take_serial_frame(
    input: &mut Vec<u8>,
    diagnostics: &mut SerialAttemptDiagnostics,
) -> Option<Vec<u8>> {
    loop {
        if input.len() > SERIAL_MAX_ENCODED_FRAME {
            let drain_to = input
                .iter()
                .position(is_line_ending)
                .map(|position| position + 1)
                .unwrap_or(input.len());
            diagnostics.oversized_lines += 1;
            input.drain(..drain_to);
        }

        let end = input.iter().position(is_line_ending)?;
        diagnostics.lines += 1;
        let mut line = input.drain(..=end).collect::<Vec<_>>();
        while line.last().is_some_and(is_line_ending) {
            line.pop();
        }
        if line.is_empty() {
            continue;
        }

        let Some(prefix_start) = find_bytes(&line, SERIAL_FRAME_PREFIX) else {
            diagnostics.noise_lines += 1;
            continue;
        };
        let hex = &line[prefix_start + SERIAL_FRAME_PREFIX.len()..];
        let Some(frame) = decode_hex_ascii(hex) else {
            diagnostics.frame_decode_errors += 1;
            continue;
        };
        if frame.len() < 3 {
            diagnostics.frame_decode_errors += 1;
            continue;
        }
        if frame[0] != SERIAL_FRAME_VERSION {
            diagnostics.version_errors += 1;
            continue;
        }
        let crc = u16::from_le_bytes([frame[1], frame[2]]);
        let body = frame[3..].to_vec();
        if crc != crc16_ccitt(&body) {
            diagnostics.crc_errors += 1;
            continue;
        }
        diagnostics.valid_frames += 1;
        return Some(body);
    }
}

#[cfg(unix)]
fn configure_serial_terminal(port_name: &str, baud: u32) -> Result<(), SerialError> {
    let status = Command::new("stty")
        .arg(stty_device_flag())
        .arg(port_name)
        .arg(baud.to_string())
        .arg("raw")
        .arg("-echo")
        .status()
        .map_err(|err| SerialError::io("run stty", err))?;
    if status.success() {
        Ok(())
    } else {
        Err(SerialError::protocol(format!("stty exited with {status}")))
    }
}

#[cfg(all(unix, target_os = "macos"))]
fn stty_device_flag() -> &'static str {
    "-f"
}

#[cfg(all(unix, not(target_os = "macos")))]
fn stty_device_flag() -> &'static str {
    "-F"
}

#[cfg(not(unix))]
fn configure_serial_terminal(_: &str, _: u32) -> Result<(), SerialError> {
    Ok(())
}

#[derive(Debug)]
enum SerialError {
    Io {
        context: &'static str,
        source: io::Error,
    },
    Port {
        context: &'static str,
        source: serialport::Error,
    },
    Protocol(String),
    Timeout,
}

#[derive(Debug)]
struct SerialFailure {
    err: SerialError,
    diagnostics: SerialDiagnostics,
}

impl fmt::Display for SerialFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "{}", self.err)?;
        write!(formatter, "{}", self.diagnostics)
    }
}

#[derive(Debug)]
struct SerialDiagnostics {
    request: &'static str,
    port_name: String,
    baud: u32,
    timeout_ms: u128,
    attempts: Vec<SerialAttemptDiagnostics>,
}

impl SerialDiagnostics {
    fn new(port_name: &str, baud: u32, request: &SerialRequest, timeout: Duration) -> Self {
        Self {
            request: request.label(),
            port_name: port_name.to_string(),
            baud,
            timeout_ms: timeout.as_millis(),
            attempts: Vec::new(),
        }
    }
}

impl fmt::Display for SerialDiagnostics {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "serial diag: wire=ascii-hex-crc-v{} request={} port={} baud={} timeout={}ms attempts={}",
            SERIAL_FRAME_VERSION,
            self.request,
            self.port_name,
            self.baud,
            self.timeout_ms,
            self.attempts.len()
        )?;
        for attempt in &self.attempts {
            writeln!(formatter, "{attempt}")?;
        }
        Ok(())
    }
}

#[derive(Debug)]
struct SerialAttemptDiagnostics {
    index: usize,
    terminal_configured: bool,
    port_opened: bool,
    dtr_low: bool,
    rts_low: bool,
    body_bytes: usize,
    frame_bytes: usize,
    read_bytes: usize,
    read_chunks: usize,
    read_timeouts: usize,
    pending_bytes: usize,
    lines: usize,
    noise_lines: usize,
    frame_decode_errors: usize,
    version_errors: usize,
    oversized_lines: usize,
    crc_errors: usize,
    valid_frames: usize,
    reply_decode_errors: usize,
    elapsed_ms: u128,
    sample: Vec<u8>,
    error: Option<String>,
}

impl SerialAttemptDiagnostics {
    fn new(index: usize) -> Self {
        Self {
            index,
            terminal_configured: false,
            port_opened: false,
            dtr_low: false,
            rts_low: false,
            body_bytes: 0,
            frame_bytes: 0,
            read_bytes: 0,
            read_chunks: 0,
            read_timeouts: 0,
            pending_bytes: 0,
            lines: 0,
            noise_lines: 0,
            frame_decode_errors: 0,
            version_errors: 0,
            oversized_lines: 0,
            crc_errors: 0,
            valid_frames: 0,
            reply_decode_errors: 0,
            elapsed_ms: 0,
            sample: Vec::new(),
            error: None,
        }
    }

    fn record_read(&mut self, bytes: &[u8]) {
        self.read_bytes += bytes.len();
        self.read_chunks += 1;
        let remaining = SERIAL_SAMPLE_BYTES.saturating_sub(self.sample.len());
        self.sample
            .extend_from_slice(&bytes[..bytes.len().min(remaining)]);
    }
}

impl fmt::Display for SerialAttemptDiagnostics {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "attempt {}: stty={} open={} dtr_low={} rts_low={} wrote={}B body={}B read={}B chunks={} read_timeouts={} lines={} noise={} valid={} frame_bad={} crc_bad={} version_bad={} oversize_line={} reply_bad={} pending={} elapsed={}ms sample_hex={} sample_ascii={}",
            self.index,
            yes_no(self.terminal_configured),
            yes_no(self.port_opened),
            yes_no(self.dtr_low),
            yes_no(self.rts_low),
            self.frame_bytes,
            self.body_bytes,
            self.read_bytes,
            self.read_chunks,
            self.read_timeouts,
            self.lines,
            self.noise_lines,
            self.valid_frames,
            self.frame_decode_errors,
            self.crc_errors,
            self.version_errors,
            self.oversized_lines,
            self.reply_decode_errors,
            self.pending_bytes,
            self.elapsed_ms,
            hex_sample(&self.sample),
            ascii_sample(&self.sample),
        )?;
        if let Some(error) = &self.error {
            write!(formatter, " error={error}")?;
        }
        Ok(())
    }
}

impl SerialError {
    fn io(context: &'static str, source: io::Error) -> Self {
        Self::Io { context, source }
    }

    fn port(context: &'static str, source: serialport::Error) -> Self {
        Self::Port { context, source }
    }

    fn protocol(message: impl Into<String>) -> Self {
        Self::Protocol(message.into())
    }

    fn is_retryable_for(&self, request: &SerialRequest) -> bool {
        request.is_retryable()
            && match self {
                Self::Io { source, .. } => is_retryable_io_error(source),
                Self::Port { source, .. } => is_retryable_port_error(source),
                Self::Timeout => true,
                Self::Protocol(_) => false,
            }
    }
}

impl fmt::Display for SerialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { context, source } => write!(formatter, "{context}: {source}"),
            Self::Port { context, source } => write!(formatter, "{context}: {source}"),
            Self::Protocol(message) => formatter.write_str(message),
            Self::Timeout => formatter.write_str("serial response timed out"),
        }
    }
}

impl SerialRequest {
    fn label(&self) -> &'static str {
        match self {
            Self::Status => "Status",
            Self::SetWifi { .. } => "SetWifi",
            Self::ClearWifi => "ClearWifi",
            Self::Command(DeviceCommand::Ping) => "Command(Ping)",
            Self::Command(DeviceCommand::SetBrightness { .. }) => "Command(SetBrightness)",
            Self::Command(DeviceCommand::CycleUsageProvider) => "Command(CycleUsageProvider)",
        }
    }

    fn is_retryable(&self) -> bool {
        match self {
            Self::Status | Self::SetWifi { .. } | Self::ClearWifi => true,
            Self::Command(DeviceCommand::Ping | DeviceCommand::SetBrightness { .. }) => true,
            Self::Command(DeviceCommand::CycleUsageProvider) => false,
        }
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn hex_sample(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "-".to_string();
    }
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn ascii_sample(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "-".to_string();
    }
    bytes
        .iter()
        .map(|byte| match byte {
            b'\n' => '|',
            b'\r' => '~',
            0x20..=0x7e => char::from(*byte),
            _ => '.',
        })
        .collect()
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

fn is_retryable_io_error(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    ) || err.raw_os_error() == Some(121)
}

fn is_retryable_port_error(err: &serialport::Error) -> bool {
    matches!(
        err.kind(),
        serialport::ErrorKind::Io(io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock)
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_semaphore_timeout_is_retryable_without_string_matching() {
        let err = SerialError::io("serial read", io::Error::from_raw_os_error(121));

        assert!(err.is_retryable_for(&SerialRequest::Status));
    }

    #[test]
    fn non_idempotent_serial_commands_are_not_retried_after_timeout() {
        let err = SerialError::Timeout;

        assert!(!err.is_retryable_for(&SerialRequest::Command(DeviceCommand::CycleUsageProvider)));
    }

    #[test]
    fn ascii_crc_frame_skips_text_noise_before_frame_prefix() {
        let body = postcard::to_allocvec(&SerialReply::Api(ApiResponse {
            ok: true,
            message: "ok".to_string(),
        }))
        .unwrap();
        let mut output = Vec::new();
        output.extend_from_slice(b"boot log without delimiter\n");
        write_serial_frame(&mut output, &body).unwrap();

        let mut diagnostics = SerialAttemptDiagnostics::new(1);
        let reply = try_read_serial_reply(&mut output, &mut diagnostics).unwrap();

        match reply {
            SerialReply::Api(response) => assert!(response.ok),
            SerialReply::Status(_) => panic!("expected api reply"),
        }
        assert!(output.is_empty());
        assert_eq!(diagnostics.noise_lines, 1);
        assert_eq!(diagnostics.valid_frames, 1);
    }

    #[test]
    fn ascii_crc_frame_rejects_corrupted_body_and_resynchronizes() {
        let bad_body = postcard::to_allocvec(&SerialReply::Api(ApiResponse {
            ok: false,
            message: "bad".to_string(),
        }))
        .unwrap();
        let good_body = postcard::to_allocvec(&SerialReply::Api(ApiResponse {
            ok: true,
            message: "good".to_string(),
        }))
        .unwrap();
        let mut input = Vec::new();
        write_serial_frame(&mut input, &bad_body).unwrap();
        let prefix_end =
            find_bytes(&input, SERIAL_FRAME_PREFIX).unwrap() + SERIAL_FRAME_PREFIX.len();
        if let Some(byte) = input[prefix_end..]
            .iter_mut()
            .find(|byte| byte.is_ascii_hexdigit())
        {
            *byte = if *byte == b'0' { b'1' } else { b'0' };
        }
        write_serial_frame(&mut input, &good_body).unwrap();

        let mut diagnostics = SerialAttemptDiagnostics::new(1);
        let reply = try_read_serial_reply(&mut input, &mut diagnostics).unwrap();

        match reply {
            SerialReply::Api(response) => assert_eq!(response.message, "good"),
            SerialReply::Status(_) => panic!("expected api reply"),
        }
        assert!(input.is_empty());
        assert!(
            diagnostics.frame_decode_errors
                + diagnostics.crc_errors
                + diagnostics.version_errors
                + diagnostics.reply_decode_errors
                > 0
        );
        assert_eq!(diagnostics.valid_frames, 1);
    }

    #[test]
    fn ascii_crc_frame_ignores_echoed_request_before_reply() {
        let request_body = postcard::to_allocvec(&SerialRequest::Status).unwrap();
        let reply_body = postcard::to_allocvec(&SerialReply::Api(ApiResponse {
            ok: true,
            message: "ok".to_string(),
        }))
        .unwrap();
        let mut input = Vec::new();
        write_serial_frame(&mut input, &request_body).unwrap();
        write_serial_frame(&mut input, &reply_body).unwrap();

        let mut diagnostics = SerialAttemptDiagnostics::new(1);
        let reply = try_read_serial_reply(&mut input, &mut diagnostics).unwrap();

        match reply {
            SerialReply::Api(response) => assert!(response.ok),
            SerialReply::Status(_) => panic!("expected api reply"),
        }
        assert!(input.is_empty());
        assert_eq!(diagnostics.reply_decode_errors, 1);
        assert_eq!(diagnostics.valid_frames, 2);
    }
}

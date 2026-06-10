pub mod config;
#[cfg(feature = "flash")]
pub mod flash;
pub mod http;
pub mod serial;
pub mod usage;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiResponse {
    pub ok: bool,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StatusResponse {
    pub connected: bool,
    pub ip: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeviceCommand {
    Ping,
    SetBrightness { value: u8 },
    CycleUsageProvider,
}

pub use config::{FlashConfig, FlashInputs, MonitorConfig, WifiCredentials};
pub use serial::SerialRequest;
pub use usage::{
    ProviderSelection, UsageCollector, UsageProvider, UsageRegistry, UsageSnapshot, UsageWindow,
};

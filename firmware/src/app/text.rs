macro_rules! ui_font_chars {
    ($text:literal) => {
        $text
    };
}

macro_rules! ui_text {
    ($text:literal) => {
        $text
    };
}

const _: &str =
    ui_font_chars!(" ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789%:./-_?");

pub const APP_TITLE: &str = ui_text!("AGENT QUOTA");
pub const WAITING: &str = ui_text!("WAITING");
pub const PUSH_USAGE_FROM_CLI: &str = ui_text!("PUSH USAGE FROM CLI");
pub const SETUP_WIFI: &str = ui_text!("SETUP WIFI");
pub const RUN_CLI_PROVISION: &str = ui_text!("RUN CLI PROVISION");
pub const WIFI_READY: &str = ui_text!("WIFI READY");
pub const NO_IP: &str = ui_text!("NO IP");
pub const WIFI_WAIT: &str = ui_text!("WIFI WAIT");
pub const CONNECTING: &str = ui_text!("CONNECTING");
pub const WAITING_FOR_QUOTA: &str = ui_text!("WAITING FOR QUOTA");
pub const USED: &str = ui_text!("USED");
pub const WINDOW_5H: &str = ui_text!("5H");
pub const WINDOW_7D: &str = ui_text!("7D");
pub const STATUS_ERR: &str = ui_text!("ERR");
pub const STATUS_EST: &str = ui_text!("EST");
pub const STATUS_LIVE: &str = ui_text!("LIVE");

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

pub const SETUP_WIFI: &str = ui_text!("Please setup Wi-Fi");
pub const NO_IP: &str = ui_text!("NO IP");
pub const CONNECTING_WIFI: &str = ui_text!("Connecting");
pub const WAITING_FOR_USAGE: &str = ui_text!("Waiting for data");
pub const WINDOW_5H: &str = ui_text!("5H");
pub const WINDOW_7D: &str = ui_text!("7D");
pub const STATUS_ERR: &str = ui_text!("ERR");
pub const STATUS_EST: &str = ui_text!("EST");

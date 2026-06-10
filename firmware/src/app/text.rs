use crate::app::ui::FontFace;

macro_rules! ui_font_chars {
    ($text:literal) => {
        $text
    };
}

macro_rules! ui_font_chars_for {
    ($font:ident, $text:literal) => {
        $text
    };
}

macro_rules! ui_text {
    ($font:ident, $text:literal) => {
        TextSpec {
            value: $text,
            font: FontFace::$font,
        }
    };
}

#[derive(Clone, Copy)]
pub struct TextSpec {
    pub value: &'static str,
    pub font: FontFace,
}

const _: &str =
    ui_font_chars!(" ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789%:./-_?");
const _: &str = ui_font_chars_for!(Galmuri11, "");

pub const SETUP_WIFI: TextSpec = ui_text!(Galmuri9, "Please setup Wi-Fi");
pub const NO_IP: TextSpec = ui_text!(Galmuri9, "NO IP");
pub const CONNECTING_WIFI: TextSpec = ui_text!(Galmuri9, "Connecting");
pub const WAITING_FOR_USAGE: TextSpec = ui_text!(Galmuri9, "Waiting for data");

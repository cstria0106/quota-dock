use crate::app::ui::FontFace;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Language {
    English,
    Korean,
}

pub const DEFAULT_LANGUAGE: Language = Language::Korean;

impl Language {
    pub fn from_code(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "en" | "en-us" => Some(Self::English),
            "ko" | "ko-kr" => Some(Self::Korean),
            _ => None,
        }
    }
}

macro_rules! ui_font_chars {
    ($text:literal) => {
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

macro_rules! ui_format {
    ($font:ident, $text:literal) => {
        $text.to_string()
    };
    ($font:ident, $text:literal, $($args:tt)+) => {
        format!($text, $($args)+)
    };
}

macro_rules! ui_format_for_fonts {
    ($first_font:ident, $second_font:ident, $text:literal) => {
        ui_format!($first_font, $text)
    };
    ($first_font:ident, $second_font:ident, $text:literal, $($args:tt)+) => {
        ui_format!($first_font, $text, $($args)+)
    };
}

#[derive(Clone, Copy)]
pub struct TextSpec {
    pub value: &'static str,
    pub font: FontFace,
}

#[derive(Clone, Copy)]
pub struct LocalizedTextSpec {
    english: TextSpec,
    korean: TextSpec,
}

impl LocalizedTextSpec {
    pub const fn get(self, language: Language) -> TextSpec {
        match language {
            Language::English => self.english,
            Language::Korean => self.korean,
        }
    }
}

const _: &str =
    ui_font_chars!(" ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789%:./-_?");

pub const SETUP_WIFI: LocalizedTextSpec = LocalizedTextSpec {
    english: ui_text!(Galmuri9, "Please setup Wi-Fi"),
    korean: ui_text!(Galmuri9, "Wi-Fi를 설정해주세요"),
};

pub const NO_IP: LocalizedTextSpec = LocalizedTextSpec {
    english: ui_text!(Galmuri9, "No IP"),
    korean: ui_text!(Galmuri9, "IP 없음"),
};

pub const CONNECTING_WIFI: LocalizedTextSpec = LocalizedTextSpec {
    english: ui_text!(Galmuri9, "Connecting"),
    korean: ui_text!(Galmuri9, "연결 중"),
};

pub const WAITING_FOR_USAGE: LocalizedTextSpec = LocalizedTextSpec {
    english: ui_text!(Galmuri9, "No provider data"),
    korean: ui_text!(Galmuri9, "프로바이더 정보 없음"),
};

pub fn reset_rolling_window(language: Language) -> String {
    match language {
        Language::English => ui_format_for_fonts!(Galmuri7, Galmuri9, "Rolling window"),
        Language::Korean => ui_format_for_fonts!(Galmuri7, Galmuri9, "롤링 윈도우"),
    }
}

pub fn reset_soon(language: Language) -> String {
    match language {
        Language::English => ui_format_for_fonts!(Galmuri7, Galmuri9, "Resets soon"),
        Language::Korean => ui_format_for_fonts!(Galmuri7, Galmuri9, "곧 초기화"),
    }
}

pub fn reset_in_days(language: Language, days: u64, hours: u64) -> String {
    match language {
        Language::English => ui_format_for_fonts!(
            Galmuri7,
            Galmuri9,
            "Resets in {days}d {hours}h",
            days = days,
            hours = hours
        ),
        Language::Korean => ui_format_for_fonts!(
            Galmuri7,
            Galmuri9,
            "{days}일 {hours}시간 후",
            days = days,
            hours = hours
        ),
    }
}

pub fn reset_in_hours(language: Language, hours: u64, minutes: u64) -> String {
    match language {
        Language::English => ui_format_for_fonts!(
            Galmuri7,
            Galmuri9,
            "Resets in {hours}h {minutes}m",
            hours = hours,
            minutes = minutes
        ),
        Language::Korean => ui_format_for_fonts!(
            Galmuri7,
            Galmuri9,
            "{hours}시간 {minutes}분 후",
            hours = hours,
            minutes = minutes
        ),
    }
}

pub fn reset_in_minutes(language: Language, minutes: u64) -> String {
    match language {
        Language::English => ui_format_for_fonts!(
            Galmuri7,
            Galmuri9,
            "Resets in {minutes}m",
            minutes = minutes
        ),
        Language::Korean => {
            ui_format_for_fonts!(Galmuri7, Galmuri9, "{minutes}분 후", minutes = minutes)
        }
    }
}

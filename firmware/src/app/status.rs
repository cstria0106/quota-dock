use crate::app::renderer::{Scene, UiObject};
use crate::app::text::{self, Language};
use crate::app::ui::{color, TextAlign, UI_HEIGHT, UI_WIDTH};
use crate::network::NetworkStatus;

pub fn waiting_scene(language: Language) -> Scene {
    usage_wait_scene(None, language)
}

pub fn network_status_scene(status: &NetworkStatus, language: Language) -> Scene {
    if !status.has_credentials {
        wifi_setup_scene(language)
    } else if status.connected {
        usage_wait_scene(status.ip.as_deref(), language)
    } else {
        wifi_connecting_scene(language)
    }
}

fn wifi_setup_scene(language: Language) -> Scene {
    let mut scene = Scene::new();
    let label = text::SETUP_WIFI.get(language);
    scene.push(UiObject::text_with_font(
        0,
        UI_HEIGHT as i32 / 2 - 16,
        UI_WIDTH as i32,
        label.value,
        label.font,
        2,
        color::TEXT,
        TextAlign::Center,
    ));
    scene
}

fn wifi_connecting_scene(language: Language) -> Scene {
    let mut scene = Scene::new();
    let label = text::CONNECTING_WIFI.get(language);
    scene.push(UiObject::text_with_font(
        0,
        88,
        UI_WIDTH as i32,
        label.value,
        label.font,
        2,
        color::TEXT,
        TextAlign::Center,
    ));
    scene.push(UiObject::loading_dots(UI_WIDTH as i32 / 2 - 34, 168));
    scene
}

fn usage_wait_scene(ip: Option<&str>, language: Language) -> Scene {
    let mut scene = Scene::new();
    let wait_label = text::WAITING_FOR_USAGE.get(language);
    scene.push(UiObject::text_with_font(
        0,
        78,
        UI_WIDTH as i32,
        wait_label.value,
        wait_label.font,
        2,
        color::TEXT,
        TextAlign::Center,
    ));
    if let Some(ip) = ip {
        scene.push(UiObject::text(
            0,
            148,
            UI_WIDTH as i32,
            ip,
            2,
            color::MINT,
            TextAlign::Center,
        ));
    } else {
        let no_ip_label = text::NO_IP.get(language);
        scene.push(UiObject::text_with_font(
            0,
            148,
            UI_WIDTH as i32,
            no_ip_label.value,
            no_ip_label.font,
            2,
            color::MINT,
            TextAlign::Center,
        ));
    }
    scene
}

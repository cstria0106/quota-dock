use crate::app::renderer::{Scene, UiObject};
use crate::app::text;
use crate::app::ui::{color, TextAlign, UI_HEIGHT, UI_WIDTH};
use crate::network::NetworkStatus;

pub fn waiting_scene() -> Scene {
    usage_wait_scene(None)
}

pub fn network_status_scene(status: &NetworkStatus) -> Scene {
    if !status.has_credentials {
        wifi_setup_scene()
    } else if status.connected {
        usage_wait_scene(status.ip.as_deref())
    } else {
        wifi_connecting_scene()
    }
}

fn wifi_setup_scene() -> Scene {
    let mut scene = Scene::new();
    scene.push(UiObject::text(
        0,
        UI_HEIGHT as i32 / 2 - 16,
        UI_WIDTH as i32,
        text::SETUP_WIFI,
        2,
        color::TEXT,
        TextAlign::Center,
    ));
    scene
}

fn wifi_connecting_scene() -> Scene {
    let mut scene = Scene::new();
    scene.push(UiObject::text(
        0,
        88,
        UI_WIDTH as i32,
        text::CONNECTING_WIFI,
        2,
        color::TEXT,
        TextAlign::Center,
    ));
    scene.push(UiObject::loading_dots(UI_WIDTH as i32 / 2 - 34, 168));
    scene
}

fn usage_wait_scene(ip: Option<&str>) -> Scene {
    let mut scene = Scene::new();
    scene.push(UiObject::text(
        0,
        78,
        UI_WIDTH as i32,
        text::WAITING_FOR_USAGE,
        2,
        color::TEXT,
        TextAlign::Center,
    ));
    scene.push(UiObject::text(
        0,
        148,
        UI_WIDTH as i32,
        ip.unwrap_or(text::NO_IP),
        2,
        color::MINT,
        TextAlign::Center,
    ));
    scene
}

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod firmware;
mod settings;
mod sync;
mod worker;

fn main() {
    app::run();
}

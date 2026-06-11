#[path = "../firmware/build_support/firmware_hash.rs"]
mod firmware_hash;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const FIRMWARE_FILES: &[&str] = &[
    "../firmware/target/flash/app.bin",
    "../firmware/target/flash/bootloader.bin",
    "../firmware/target/flash/firmware-metadata.env",
    "../firmware/target/flash/partition-table.bin",
];

fn main() {
    for path in FIRMWARE_FILES {
        println!("cargo:rerun-if-changed={path}");
        if !Path::new(path).is_file() {
            panic!(
                "missing bundled firmware artifact {path}; run `cd firmware && ./scripts/build.sh` before building the desktop app"
            );
        }
    }
    let firmware_dir = PathBuf::from("../firmware");
    assert_firmware_images_current(&firmware_dir);
    // The metadata copied by firmware/scripts/build.sh is the source of truth.
    // Recomputing here only registers Cargo change tracking for firmware inputs.
    let _ = firmware_hash::firmware_source_hash(&firmware_dir);
    let metadata = read_firmware_metadata(&firmware_dir.join("target/flash/firmware-metadata.env"));
    println!(
        "cargo:rustc-env=QUOTA_DOCK_FIRMWARE_VERSION={}",
        metadata.version
    );
    println!("cargo:rustc-env=QUOTA_DOCK_FIRMWARE_HASH={}", metadata.hash);
    tauri_build::build();
}

fn assert_firmware_images_current(firmware_dir: &Path) {
    let app_bin = firmware_dir.join("target/flash/app.bin");
    let metadata = firmware_dir.join("target/flash/firmware-metadata.env");
    let app_modified = modified_at(&app_bin);
    if modified_at(&metadata) < app_modified {
        panic!(
            "bundled firmware metadata is older than {}; run `cd firmware && ./scripts/build.sh`",
            app_bin.display()
        );
    }
    for source in firmware_hash::firmware_hash_input_paths(firmware_dir) {
        if modified_at(&source) > app_modified {
            panic!(
                "bundled firmware is older than {}; run `cd firmware && ./scripts/build.sh`",
                source.display()
            );
        }
    }
}

fn modified_at(path: &Path) -> SystemTime {
    path.metadata()
        .unwrap_or_else(|err| panic!("read metadata {}: {err}", path.display()))
        .modified()
        .unwrap_or_else(|err| panic!("read modified time {}: {err}", path.display()))
}

fn read_firmware_metadata(path: &Path) -> FirmwareMetadata {
    let contents = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("read firmware metadata {}: {err}", path.display()));
    let mut version = None;
    let mut hash = None;
    for line in contents.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "version" => version = non_empty(value),
            "hash" => hash = non_empty(value),
            _ => {}
        }
    }
    FirmwareMetadata {
        version: version
            .unwrap_or_else(|| panic!("firmware metadata {} is missing version", path.display())),
        hash: hash
            .unwrap_or_else(|| panic!("firmware metadata {} is missing hash", path.display())),
    }
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

struct FirmwareMetadata {
    version: String,
    hash: String,
}

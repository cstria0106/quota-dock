use std::fs;
use std::path::{Path, PathBuf};

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;
const HASH_ROOTS: &[&str] = &[
    "Cargo.lock",
    "Cargo.toml",
    "assets/fonts",
    "build.rs",
    "build_support",
    "partitions.csv",
    "rust-toolchain.toml",
    "scripts/build.sh",
    "sdkconfig.defaults",
    "src",
];

pub fn firmware_source_hash(firmware_dir: &Path) -> String {
    let inputs = firmware_hash_input_paths(firmware_dir);
    for path in watched_paths(firmware_dir, &inputs) {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    let mut hash = FNV_OFFSET_BASIS;
    for path in inputs {
        let relative = path
            .strip_prefix(firmware_dir)
            .expect("hash input should be under firmware dir");
        let label = relative.to_string_lossy().replace('\\', "/");
        hash = fnv1a(hash, label.as_bytes());
        hash = fnv1a(hash, &[0]);
        match fs::read(&path) {
            Ok(bytes) => hash = fnv1a(hash, &bytes),
            Err(err) => panic!("read hash source {}: {err}", path.display()),
        }
        hash = fnv1a(hash, &[0xff]);
    }
    for feature in enabled_features() {
        hash = fnv1a(hash, b"feature:");
        hash = fnv1a(hash, feature.as_bytes());
        hash = fnv1a(hash, &[0xff]);
    }
    format!("{hash:016x}")
}

pub fn firmware_hash_input_paths(firmware_dir: &Path) -> Vec<PathBuf> {
    let mut inputs = Vec::new();
    for root in HASH_ROOTS {
        collect_files(firmware_dir, Path::new(root), &mut inputs);
    }
    inputs.sort();
    inputs
}

fn watched_paths(firmware_dir: &Path, inputs: &[PathBuf]) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = HASH_ROOTS
        .iter()
        .map(|root| firmware_dir.join(root))
        .chain(inputs.iter().cloned())
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

fn collect_files(firmware_dir: &Path, relative: &Path, inputs: &mut Vec<PathBuf>) {
    let path = firmware_dir.join(relative);
    let metadata = fs::metadata(&path)
        .unwrap_or_else(|err| panic!("read hash source metadata {}: {err}", path.display()));
    if metadata.is_file() {
        inputs.push(path);
        return;
    }
    if !metadata.is_dir() {
        panic!("hash source is not a file or directory: {}", path.display());
    }

    let mut children: Vec<PathBuf> = fs::read_dir(&path)
        .unwrap_or_else(|err| panic!("read hash source directory {}: {err}", path.display()))
        .map(|entry| {
            entry
                .unwrap_or_else(|err| panic!("read hash source entry {}: {err}", path.display()))
                .path()
        })
        .collect();
    children.sort();

    for child in children {
        let child_relative = child
            .strip_prefix(firmware_dir)
            .expect("hash child should be under firmware dir");
        collect_files(firmware_dir, child_relative, inputs);
    }
}

fn enabled_features() -> Vec<String> {
    let mut features: Vec<String> = std::env::vars()
        .filter_map(|(key, value)| {
            if key.starts_with("CARGO_FEATURE_") && value == "1" {
                Some(key)
            } else {
                None
            }
        })
        .collect();
    features.sort();
    features
}

fn fnv1a(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

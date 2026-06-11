use std::path::Path;

const FIRMWARE_FILES: &[&str] = &[
    "../firmware/target/flash/app.bin",
    "../firmware/target/flash/bootloader.bin",
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
}

# Project Notes

This repository is organized as a multi-target project:

- `firmware/` contains the Rust-first ESP-IDF firmware for an ESP32-S3 board with an SH8601 AMOLED display.
- `desktop/` contains the Tauri v2 desktop app and its Vanilla TypeScript UI.

# Tooling

Prefer project-local tooling where practical.

- ESP-IDF is set up automatically under `firmware/.tools/espressif/`.
- Local helper binaries such as `ldproxy` and `espflash` are installed under `firmware/.tools/bin/`.
- Do not depend on `/opt/esp-idf` for this project.
- The Rust ESP toolchain and Cargo cache may still live in the user's normal Rust locations, such as `~/.rustup` and `~/.cargo`.

Use the scripts in `firmware/scripts/` instead of calling ESP-IDF tooling directly. They prepare the project-local environment before running commands.

# Build

Run builds from the firmware root:

```sh
cd firmware
./scripts/build.sh
```

The project target is `esp32s3`, and the default flash settings are kept in `sdkconfig.defaults`.

Desktop builds are run from the desktop root:

```sh
cd desktop
pnpm typecheck
cargo test --locked
pnpm build
```

Windows desktop builds should be validated with `cargo xwin` through the project script:

```sh
cd desktop
pnpm build:windows
```

Keep Windows builds on Tauri's `custom-protocol` feature so the portable `.exe` loads bundled UI assets instead of trying to open the dev server URL. Desktop release artifacts should be single portable binaries, not installer packages.

# Upload And Monitor

The board is expected on `/dev/ttyACM0`.

```sh
cd firmware
./scripts/flash.sh
./scripts/monitor.sh
```

To use a different serial port, pass it as the first argument:

```sh
cd firmware
./scripts/flash.sh /dev/ttyUSB0
./scripts/monitor.sh /dev/ttyUSB0
```

# Generated Files

`firmware/.tools/`, `firmware/target/`, `firmware/sdkconfig.cargo`, and `firmware/sdkconfig` are generated and should not be edited by hand.

#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IDF_VERSION="v6.0.1"
ESPRESSIF_DIR="$ROOT_DIR/.tools/espressif"
LOCAL_IDF_PATH="$ESPRESSIF_DIR/esp-idf/$IDF_VERSION"
LOCAL_IDF_TOOLS_PATH="$ESPRESSIF_DIR/tools"
PARTITION_DEFAULTS="$ROOT_DIR/.tools/sdkconfig.partition.defaults"

require_commands() {
    local missing=()

    for command in "$@"; do
        if ! command -v "$command" >/dev/null 2>&1; then
            missing+=("$command")
        fi
    done

    if [[ ${#missing[@]} -gt 0 ]]; then
        printf "Missing required command(s): %s\n" "${missing[*]}" >&2
        printf "Install them first, then rerun %s.\n" "$0" >&2
        exit 1
    fi
}

require_commands cmake ninja

if [[ -f /home/goorm/export-esp.sh ]]; then
    source /home/goorm/export-esp.sh
fi

cd "$ROOT_DIR"
if [[ ! -x "$ROOT_DIR/.tools/bin/ldproxy" ]]; then
    cargo install ldproxy --root "$ROOT_DIR/.tools"
fi
if [[ ! -x "$ROOT_DIR/.tools/bin/espflash" ]]; then
    cargo install espflash --root "$ROOT_DIR/.tools"
fi
export PATH="$ROOT_DIR/.tools/bin:$PATH"

PARTITION_DEFAULTS_CONTENT=$(cat <<EOF
CONFIG_PARTITION_TABLE_CUSTOM_FILENAME="$ROOT_DIR/partitions.csv"
CONFIG_PARTITION_TABLE_FILENAME="$ROOT_DIR/partitions.csv"
EOF
)

if [[ ! -f "$PARTITION_DEFAULTS" ]] || [[ "$(cat "$PARTITION_DEFAULTS")" != "$PARTITION_DEFAULTS_CONTENT" ]]; then
    printf "%s\n" "$PARTITION_DEFAULTS_CONTENT" >"$PARTITION_DEFAULTS"
fi

if [[ ! -f "$LOCAL_IDF_PATH/export.sh" ]]; then
    mkdir -p "$ESPRESSIF_DIR/esp-idf"
    git clone --branch "$IDF_VERSION" --recursive https://github.com/espressif/esp-idf.git "$LOCAL_IDF_PATH"
fi

export IDF_PATH="$LOCAL_IDF_PATH"
export IDF_TOOLS_PATH="$LOCAL_IDF_TOOLS_PATH"
export ESP_IDF_SDKCONFIG="$ROOT_DIR/sdkconfig.cargo"
export ESP_IDF_SDKCONFIG_DEFAULTS="$ROOT_DIR/sdkconfig.defaults;$PARTITION_DEFAULTS"

if [[ ! -d "$LOCAL_IDF_TOOLS_PATH" ]]; then
    "$LOCAL_IDF_PATH/install.sh" esp32s3
fi

source "$LOCAL_IDF_PATH/export.sh"

cargo_args=(+esp build --release)
if [[ -n "${FIRMWARE_FEATURES:-}" ]]; then
    cargo_args+=(--features "$FIRMWARE_FEATURES")
fi
cargo "${cargo_args[@]}"

FLASH_DIR="$ROOT_DIR/target/flash"
RELEASE_DIR="$ROOT_DIR/target/xtensa-esp32s3-espidf/release"
APP_ELF="$RELEASE_DIR/quota-dock-firmware"
APP_BIN="$FLASH_DIR/app.bin"
BOOTLOADER_BIN="$RELEASE_DIR/bootloader.bin"
PARTITION_TABLE_BIN="$FLASH_DIR/partition-table.bin"

mkdir -p "$FLASH_DIR"
"$ROOT_DIR/.tools/bin/espflash" partition-table \
    --to-binary \
    --output "$PARTITION_TABLE_BIN" \
    "$ROOT_DIR/partitions.csv" >/dev/null
"$ROOT_DIR/.tools/bin/espflash" save-image \
    --chip esp32s3 \
    --flash-size 16mb \
    --flash-mode qio \
    --flash-freq 80mhz \
    --partition-table "$ROOT_DIR/partitions.csv" \
    --bootloader "$BOOTLOADER_BIN" \
    "$APP_ELF" \
    "$APP_BIN" >/dev/null
cp "$BOOTLOADER_BIN" "$FLASH_DIR/bootloader.bin"

printf "Flash binaries written to %s\n" "$FLASH_DIR"

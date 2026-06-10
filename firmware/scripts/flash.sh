#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${1:-/dev/ttyACM0}"
MONITOR="${MONITOR:-1}"

cd "$ROOT_DIR"
ESPFLASH="$ROOT_DIR/.tools/bin/espflash"
if [[ ! -x "$ESPFLASH" ]]; then
    cargo install espflash --root "$ROOT_DIR/.tools"
fi

PARTITION_TABLE="$ROOT_DIR/target/partition-table.bin"
BOOTLOADER="$ROOT_DIR/target/xtensa-esp32s3-espidf/release/bootloader.bin"
"$ESPFLASH" partition-table --to-binary --output "$PARTITION_TABLE" "$ROOT_DIR/partitions.csv" >/dev/null

if [[ "$MONITOR" == "1" ]]; then
    "$ESPFLASH" flash --port "$PORT" target/xtensa-esp32s3-espidf/release/agent-quota-monitor
    "$ESPFLASH" write-bin --port "$PORT" 0x0 "$BOOTLOADER"
    "$ESPFLASH" write-bin --port "$PORT" 0x8000 "$PARTITION_TABLE"
    "$ESPFLASH" monitor --port "$PORT"
else
    "$ESPFLASH" flash --port "$PORT" target/xtensa-esp32s3-espidf/release/agent-quota-monitor
    "$ESPFLASH" write-bin --port "$PORT" 0x0 "$BOOTLOADER"
    "$ESPFLASH" write-bin --port "$PORT" 0x8000 "$PARTITION_TABLE"
fi

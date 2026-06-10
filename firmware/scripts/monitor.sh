#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${1:-/dev/ttyACM0}"

cd "$ROOT_DIR"
ESPFLASH="$ROOT_DIR/.tools/bin/espflash"
if [[ ! -x "$ESPFLASH" ]]; then
    cargo install espflash --root "$ROOT_DIR/.tools"
fi

"$ESPFLASH" monitor --port "$PORT"

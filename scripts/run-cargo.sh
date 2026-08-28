#!/usr/bin/env bash

set -euo pipefail

CARGO_PATH="$(rustup which cargo)"
RUST_BIN_DIR="${CARGO_PATH%/cargo}"

export PATH="$RUST_BIN_DIR:$PATH"
exec "$CARGO_PATH" "$@"

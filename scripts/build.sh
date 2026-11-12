#!/usr/bin/env sh
set -eu
. "$(dirname -- "$0")/dev-env.sh"
cargo build --release --locked --workspace
#!/usr/bin/env sh
set -eu
. "$(dirname -- "$0")/dev-env.sh"
cargo test --locked --workspace
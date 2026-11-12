#!/usr/bin/env sh
set -eu
. "$(dirname -- "$0")/dev-env.sh"
cargo run --release --locked -p fleece-render-server --bin fleece-render
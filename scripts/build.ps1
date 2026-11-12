$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/dev-env.ps1"
Push-Location (Split-Path -Parent $PSScriptRoot)
try {
    cargo build --release --locked --workspace
} finally {
    Pop-Location
}
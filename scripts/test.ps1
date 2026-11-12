$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/dev-env.ps1"
Push-Location (Split-Path -Parent $PSScriptRoot)
try {
    cargo test --locked --workspace
} finally {
    Pop-Location
}
param(
    [int]$Port = 57680
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/dev-env.ps1"
Push-Location (Split-Path -Parent $PSScriptRoot)
try {
    $env:FLEECE_RENDER_PORT = "$Port"
    cargo run --release --locked -p fleece-render-server --bin fleece-render
} finally {
    Pop-Location
}
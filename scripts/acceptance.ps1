param(
    [switch]$Online,
    [switch]$Gpu,
    [int]$Port = 57681
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/dev-env.ps1"
$Root = Split-Path -Parent $PSScriptRoot
Push-Location $Root
$server = $null
try {
    $backend = if ($Gpu) { 'gpu' } else { 'cpu' }
    $features = @()
    $oldRequireGpu = $env:FLEECE_RENDER_REQUIRE_GPU
    if ($Gpu) {
        $features = @('--features', 'gpu')
        $env:FLEECE_RENDER_REQUIRE_GPU = '1'
    }
    Write-Host '[1/5] Running locked Rust test suite...'
    cargo test --locked --workspace @features
    if ($LASTEXITCODE -ne 0) { throw 'cargo test failed' }

    Write-Host '[2/5] Building release binary...'
    cargo build --release --locked -p fleece-render-server --bin fleece-render @features
    if ($LASTEXITCODE -ne 0) { throw 'release build failed' }

    $stamp = Get-Date -Format 'yyyy-MM-dd_HHmmss'
    $out = Join-Path $Root ("output\acceptance\$stamp")
    New-Item -ItemType Directory -Force -Path $out | Out-Null
    $oldPort = $env:FLEECE_RENDER_PORT
    $oldRustLog = $env:RUST_LOG
    $oldLogFile = $env:FLEECE_RENDER_LOG_FILE
    $env:FLEECE_RENDER_PORT = "$Port"
    $env:RUST_LOG = 'info'
    $env:FLEECE_RENDER_LOG_FILE = (Join-Path $out 'fleece-render.log')
    $targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
    $binary = Join-Path $targetDir 'release\fleece-render.exe'
    $server = Start-Process `
        -FilePath (Resolve-Path -LiteralPath $binary).Path `
        -ArgumentList @('--host', '127.0.0.1', '--backend', $backend) `
        -WorkingDirectory $Root `
        -RedirectStandardOutput (Join-Path $out 'server.stdout.log') `
        -RedirectStandardError (Join-Path $out 'server.stderr.log') `
        -PassThru -WindowStyle Hidden
    if ($null -ne $oldPort) { $env:FLEECE_RENDER_PORT = $oldPort } else { Remove-Item Env:FLEECE_RENDER_PORT -ErrorAction SilentlyContinue }
    if ($null -ne $oldRustLog) { $env:RUST_LOG = $oldRustLog } else { Remove-Item Env:RUST_LOG -ErrorAction SilentlyContinue }
    if ($null -ne $oldLogFile) { $env:FLEECE_RENDER_LOG_FILE = $oldLogFile } else { Remove-Item Env:FLEECE_RENDER_LOG_FILE -ErrorAction SilentlyContinue }

    $base = "http://127.0.0.1:$Port"
    $ready = $false
    for ($i = 0; $i -lt 50; $i++) {
        Start-Sleep -Milliseconds 200
        try {
            $health = Invoke-RestMethod -UseBasicParsing -TimeoutSec 2 "$base/health"
            if ($health.ok -eq $true) { $ready = $true; break }
        } catch { }
    }
    if (-not $ready) { throw "server did not become ready on $base" }
    if ($health.backend -ne $backend) { throw "Expected $backend, got $($health.backend)" }
    Write-Host "[3/5] Server ready at $base (backend=$($health.backend))"

    $skin = (Resolve-Path 'crates/fleece-render-core/tests/fixtures/source_skin.png').Path
    $cape = (Resolve-Path 'crates/fleece-render-core/tests/fixtures/source_cape.png').Path
    $skinB64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($skin))
    $options = 'definition=1.5&transparent=false&showName=true&nameTag=LanYangYang_CN'
    $escapedSkin = [Uri]::EscapeDataString($skin)
    $escapedCape = [Uri]::EscapeDataString($cape)
    $urlUri = "$base/url/image/both?$options&skinUrl=$escapedSkin&capeUrl=$escapedCape"
    $dataUri = "$base/url/image/both?$options&skinData=$([Uri]::EscapeDataString($skinB64))&capeUrl=$escapedCape"
    $rawUri = "$base/raw/image/both?$options&capeUrl=$escapedCape"

    $results = @{}
    foreach ($case in @(
        @{Name='url'; Uri=$urlUri; Method='GET'},
        @{Name='data'; Uri=$dataUri; Method='GET'},
        @{Name='raw'; Uri=$rawUri; Method='POST'}
    )) {
        $sw = [Diagnostics.Stopwatch]::StartNew()
        if ($case.Method -eq 'POST') {
            $response = Invoke-WebRequest -UseBasicParsing -TimeoutSec 30 -Method Post -ContentType 'image/png' -InFile $skin $case.Uri
        } else {
            $response = Invoke-WebRequest -UseBasicParsing -TimeoutSec 30 $case.Uri
        }
        $sw.Stop()
        if ($response.StatusCode -ne 200) { throw "HTTP $($case.Name) failed: $($response.StatusCode)" }
        if ($response.Headers['X-Fleece-Render-Backend'] -ne $backend) { throw "HTTP $($case.Name) did not use $backend" }
        if ($sw.Elapsed.TotalSeconds -ge 5.0) { throw "HTTP $($case.Name) exceeded 5s: $($sw.Elapsed.TotalSeconds)" }
        $path = Join-Path $out "api_$($case.Name).png"
        [IO.File]::WriteAllBytes($path, $response.Content)
        $results[$case.Name] = (Get-FileHash -Algorithm SHA256 $path).Hash
        Write-Host ("  [ OK ] {0,-4} {1,6:N3}s  {2}" -f $case.Name, $sw.Elapsed.TotalSeconds, $response.Headers['X-Fleece-Render-Size'])
    }
    if (-not ($results['url'] -eq $results['data'] -and $results['data'] -eq $results['raw'])) {
        throw 'url/data/raw outputs are not byte-identical'
    }
    Write-Host "[4/5] URL, data URL and raw PNG output hashes match"

    foreach ($transparent in @('true', 'false')) {
        $avatarOptions = "style=avatar&transparent=$transparent"
        $dataUrl = [Uri]::EscapeDataString("data:image/png;base64,$skinB64")
        $avatarHashes = foreach ($case in @(
            @{Name='path'; Uri="$base/url/image/front?$avatarOptions&skinUrl=$escapedSkin"},
            @{Name='base64'; Uri="$base/url/image/front?$avatarOptions&skinData=$([Uri]::EscapeDataString($skinB64))"},
            @{Name='data-url'; Uri="$base/url/image/front?$avatarOptions&skinData=$dataUrl"},
            @{Name='raw'; Uri="$base/raw/image/front?$avatarOptions"}
        )) {
            $sw = [Diagnostics.Stopwatch]::StartNew()
            if ($case.Name -eq 'raw') {
                $response = Invoke-WebRequest -UseBasicParsing -TimeoutSec 5 -Method Post -ContentType 'image/png' -InFile $skin $case.Uri
            } else {
                $response = Invoke-WebRequest -UseBasicParsing -TimeoutSec 5 $case.Uri
            }
            $sw.Stop()
            if ($response.StatusCode -ne 200 -or $response.Headers['X-Fleece-Render-Size'] -ne '64x64' -or $response.Headers['X-Fleece-Render-Backend'] -ne 'cpu') {
                throw "Invalid avatar response for $($case.Name) (transparent=$transparent)"
            }
            if ($sw.Elapsed.TotalSeconds -ge 5.0) { throw "Avatar $($case.Name) exceeded 5s: $($sw.Elapsed.TotalSeconds)" }
            $path = Join-Path $out "avatar_$($transparent)_$($case.Name).png"
            [IO.File]::WriteAllBytes($path, $response.Content)
            (Get-FileHash -Algorithm SHA256 $path).Hash
        }
        if (@($avatarHashes | Select-Object -Unique).Count -ne 1) {
            throw "Avatar input outputs are not byte-identical (transparent=$transparent)"
        }
        Write-Host "  [ OK ] 64x64 avatar path/Base64/data URL/raw hashes match (transparent=$transparent)"
    }

    if ($Online) {
        Write-Host '  [ .. ] Running direct Mojang name/UUID acceptance...'
        $nameSw = [Diagnostics.Stopwatch]::StartNew()
        $byName = Invoke-WebRequest -UseBasicParsing -TimeoutSec 60 "$base/mojang/image/front?name=LanYangYang_CN&definition=1.5&showName=true"
        $nameSw.Stop()
        $byUuid = Invoke-WebRequest -UseBasicParsing -TimeoutSec 60 "$base/mojang/image/front?name=cd78ace69f64479182d3718446d09157&definition=1.5&showName=true"
        $namePath = Join-Path $out 'mojang_by_name.png'
        $uuidPath = Join-Path $out 'mojang_by_uuid.png'
        [IO.File]::WriteAllBytes($namePath, $byName.Content)
        [IO.File]::WriteAllBytes($uuidPath, $byUuid.Content)
        if ((Get-FileHash $namePath).Hash -ne (Get-FileHash $uuidPath).Hash) {
            throw 'Mojang name and UUID renders differ'
        }
        Write-Host ("  [ OK ] name/UUID identical; network-inclusive name request {0:N3}s" -f $nameSw.Elapsed.TotalSeconds)
    }

    Write-Host "[5/5] Acceptance passed. Artifacts: $out"
} finally {
    if ($null -ne $server -and -not $server.HasExited) {
        Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
        Write-Host 'Acceptance server stopped.'
    }
    if ($null -ne $oldRequireGpu) { $env:FLEECE_RENDER_REQUIRE_GPU = $oldRequireGpu } else { Remove-Item Env:FLEECE_RENDER_REQUIRE_GPU -ErrorAction SilentlyContinue }
    Pop-Location
}
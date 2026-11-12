# Source this file from another PowerShell script:
#   . "$PSScriptRoot/dev-env.ps1"
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot

if (Test-Path (Join-Path $Root '.cargo\bin\cargo.exe')) {
    $env:CARGO_HOME = Join-Path $Root '.cargo'
    $env:RUSTUP_HOME = Join-Path $Root '.rustup'
    $env:PATH = "$(Join-Path $Root '.cargo\bin');$env:PATH"
}

$llvm = Get-ChildItem (Join-Path $Root '.toolchains') -Directory -Filter 'llvm-mingw-*-x86_64*' -ErrorAction SilentlyContinue |
    Sort-Object Name -Descending |
    Select-Object -First 1
if ($null -ne $llvm) {
    $llvmBin = Join-Path $llvm.FullName 'bin'
    $env:PATH = "$llvmBin;$env:PATH"
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = 'x86_64-w64-mingw32-clang'
    $env:CC_x86_64_pc_windows_gnu = 'x86_64-w64-mingw32-clang'
    $env:AR_x86_64_pc_windows_gnu = 'llvm-ar'

    # Rust's GNU target asks for libgcc/libgcc_eh. LLVM-MinGW provides the
    # equivalent compiler-rt builtins and libunwind. Keep compatibility names
    # inside the local toolchain directory.
    $shim = Join-Path $Root '.toolchains\gcc-shim'
    New-Item -ItemType Directory -Force -Path $shim | Out-Null
    $builtins = Join-Path $llvm.FullName 'lib\clang\23\lib\windows\libclang_rt.builtins-x86_64.a'
    $unwind = Join-Path $llvm.FullName 'x86_64-w64-mingw32\lib\libunwind.a'
    if (Test-Path $builtins) {
        Copy-Item -LiteralPath $builtins -Destination (Join-Path $shim 'libgcc.a') -Force
    }
    if (Test-Path $unwind) {
        Copy-Item -LiteralPath $unwind -Destination (Join-Path $shim 'libgcc_eh.a') -Force
    }
    $nativeFlag = "-Lnative=$shim"
    if ($env:RUSTFLAGS -and -not $env:RUSTFLAGS.Contains($nativeFlag)) {
        $env:RUSTFLAGS = "$env:RUSTFLAGS $nativeFlag"
    } elseif (-not $env:RUSTFLAGS) {
        $env:RUSTFLAGS = $nativeFlag
    }
}

$tmp = Join-Path $Root '.tmp'
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
$env:TMP = $tmp
$env:TEMP = $tmp
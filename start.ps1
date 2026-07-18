# Arcadia OS - Build and Run Script for Windows (PowerShell)
# Usage: .\start.ps1 [build|run|clean]

param(
    [Parameter(Position=0)]
    [ValidateSet("build", "run", "clean", "help")]
    [string]$Command = "run"
)

$ProjectDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$KernelDir  = Join-Path $ProjectDir "kernel"
$BuildDir   = Join-Path $ProjectDir "build"
$TargetDir  = Join-Path $ProjectDir "target\x86_64-unknown-none\release"
$RustUp     = Join-Path $env:USERPROFILE ".rustup\toolchains\nightly-x86_64-pc-windows-msvc"
$RustLLD    = Join-Path $RustUp "lib\rustlib\x86_64-pc-windows-msvc\bin\rust-lld.exe"

function Write-Info  { param([string]$Msg) Write-Host "[INFO] $Msg" -ForegroundColor Cyan }
function Write-OK    { param([string]$Msg) Write-Host "[  OK] $Msg" -ForegroundColor Green }
function Write-Warn  { param([string]$Msg) Write-Host "[WARN] $Msg" -ForegroundColor Yellow }
function Write-Fail  { param([string]$Msg) Write-Host "[FAIL] $Msg" -ForegroundColor Red }

function Test-Command {
    param([string]$Name)
    return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

# -- Build kernel -------------------------------------------------------------
function Build-Kernel {
    Write-Info "Building kernel (cargo build --release)..."
    Push-Location $ProjectDir
    $output = & cargo build --target x86_64-unknown-none --release -p arcadia-kernel 2>&1
    Pop-Location
    if ($LASTEXITCODE -ne 0) {
        $output | ForEach-Object { Write-Host $_ }
        throw "cargo build failed"
    }
    Write-OK "Kernel library built."
}

# -- Assemble PVH boot stub ---------------------------------------------------
function Build-BootStub {
    Write-Info "Assembling boot64.asm (PVH boot stub)..."

    $asmFile = Join-Path $KernelDir "src\arch\boot64.asm"
    $objFile = Join-Path $BuildDir "boot64.o"

    if (-not (Test-Command "nasm")) {
        throw "nasm not found. Install NASM:  winget install nasm"
    }

    & nasm -f elf64 $asmFile -o $objFile 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "nasm failed" }
    Write-OK "Boot stub assembled."
}

# -- Link into ELF binary -----------------------------------------------------
function Build-Link {
    Write-Info "Linking kernel ELF with rust-lld..."

    if (-not (Test-Path $RustLLD)) {
        throw "rust-lld not found at: $RustLLD"
    }

    $ldScript = Join-Path $ProjectDir "linker.ld"
    $bootObj  = Join-Path $BuildDir "boot64.o"
    $kernelA  = Join-Path $TargetDir "libarcadia_kernel.a"
    $outElf   = Join-Path $BuildDir "arcadia-kernel.elf"

    & $RustLLD -flavor gnu -m elf_x86_64 -T $ldScript -o $outElf $bootObj $kernelA --nmagic 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "linking failed" }

    $size = (Get-Item $outElf).Length
    Write-OK "Kernel ELF: $outElf  ($([math]::Round($size/1024)) KiB)"
}

# -- Run in QEMU --------------------------------------------------------------
function Start-QEMU {
    $elf = Join-Path $BuildDir "arcadia-kernel.elf"

    if (-not (Test-Path $elf)) {
        Write-Fail "Kernel ELF not found: $elf"
        return
    }

    $qemu = "qemu-system-x86_64"
    if (-not (Test-Command $qemu)) {
        Write-Fail "qemu-system-x86_64 not found."
        Write-Info "Install:  winget install XP890JNNBH98460"
        return
    }

    Write-Host ""
    Write-Info "Starting QEMU (PVH boot)..."
    Write-Info "  Kernel : $elf"
    Write-Info "  RAM    : 256 MiB"
    Write-Info "  Serial : stdio (see output below)"
    Write-Info "  Display: none (serial only)"
    Write-Info "  Tip: Press Ctrl-C to exit QEMU"
    Write-Host ""
    Write-Host "--- QEMU Output --------------------------------------------------" -ForegroundColor DarkGray
    Write-Host ""

    & $qemu `
        -kernel $elf `
        -m 256M `
        -nographic `
        -drive file=build\test-fat32.img,format=raw,if=ide `
        -no-reboot

    Write-Host ""
    Write-Host "--- QEMU Exited --------------------------------------------------" -ForegroundColor DarkGray
}

# -- Clean --------------------------------------------------------------------
function Clear-Build {
    Write-Info "Cleaning build artifacts..."
    Push-Location $ProjectDir
    & cargo clean 2>$null
    Pop-Location
    if (Test-Path $BuildDir) {
        Remove-Item -Recurse -Force $BuildDir -ErrorAction SilentlyContinue
    }
    Write-OK "Clean complete."
}

# -- Help ---------------------------------------------------------------------
function Show-Help {
    Write-Host ""
    Write-Host "  Arcadia OS - Build and Run" -ForegroundColor Cyan
    Write-Host "  ========================"
    Write-Host ""
    Write-Host "  Usage:  .\start.ps1 [command]"
    Write-Host ""
    Write-Host "  Commands:"
    Write-Host "    run     Build and run in QEMU   (default)"
    Write-Host "    build   Build only"
    Write-Host "    clean   Remove build artifacts"
    Write-Host "    help    Show this help"
    Write-Host ""
    Write-Host "  Boot method: QEMU PVH (XEN_ELFNOTE_PHYS32_ENTRY)" -ForegroundColor Yellow
    Write-Host ""
    Write-Host "  Prerequisites:" -ForegroundColor Yellow
    Write-Host "    - Rust nightly toolchain with x86_64-unknown-none target"
    Write-Host "    - NASM assembler"
    Write-Host "    - QEMU (qemu-system-x86_64)"
    Write-Host ""
}

# -- Main ---------------------------------------------------------------------
New-Item -ItemType Directory -Force -Path $BuildDir | Out-Null

switch ($Command) {
    "build" {
        Build-Kernel
        Build-BootStub
        Build-Link
        Write-Host ""
        Write-OK "Build complete."
    }
    "run" {
        Build-Kernel
        Build-BootStub
        Build-Link
        Start-QEMU
    }
    "clean" { Clear-Build }
    "help"  { Show-Help }
}

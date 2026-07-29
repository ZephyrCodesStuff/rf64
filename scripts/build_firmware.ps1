# PowerShell Build & Binary Extraction Script for MIDI Fighter 64 (rf64)
# Run from the project root directory (where Cargo.toml is located)

param (
    [string]$Profile = "debug"
)

Write-Host "Building rf64 firmware (Profile: $Profile)..." -ForegroundColor Cyan

if ($Profile -eq "release") {
    cargo build --release
    $ElfPath = "target/avr-none/release/rf64.elf"
    $BinPath = "target/rf64_release.bin"
    $HexPath = "target/rf64_release.hex"
} else {
    cargo build
    $ElfPath = "target/avr-none/debug/rf64.elf"
    $BinPath = "target/rf64.bin"
    $HexPath = "target/rf64.hex"
}

if (-not (Test-Path $ElfPath)) {
    Write-Error "Build failed or ELF binary not found at $ElfPath"
    exit 1
}

# Locate avr-objcopy on the system
$ObjCopy = (Get-Command avr-objcopy -ErrorAction SilentlyContinue).Path
if (-not $ObjCopy) {
    # Fallback to known path if not in env PATH
    $ObjCopy = "C:\Users\zeph\Applicazioni\avr-gcc-16.1.0-x64-windows\bin\avr-objcopy.exe"
}

if (-not (Test-Path $ObjCopy)) {
    Write-Error "avr-objcopy.exe was not found."
    exit 1
}

Write-Host "Extracting raw binary firmware ($BinPath)..." -ForegroundColor Green
& $ObjCopy -O binary -R .eeprom -R .fuse -R .lock -R .signature -R .user_signatures -R .noinit -R .bss $ElfPath $BinPath

Write-Host "Extracting Intel HEX firmware ($HexPath)..." -ForegroundColor Green
& $ObjCopy -O ihex -R .eeprom -R .fuse -R .lock -R .signature -R .user_signatures -R .noinit -R .bss $ElfPath $HexPath

Write-Host "`nFirmware Build Successful!" -ForegroundColor Green
Get-Item $ElfPath, $BinPath, $HexPath | Select-Object Name, Length, LastWriteTime | Format-Table -AutoSize

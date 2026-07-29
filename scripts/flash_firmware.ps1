# PowerShell Script to Flash Firmware using dfu-programmer
# Run from the project root directory (where Cargo.toml is located)

# ICSP programming with Bus Pirate:
# avrdude -p ATmega32u4 -c buspirate -P COM3    -U flash:w:bin/BootloaderDFU_mf64.hex
# avrdude -p ATmega32u4 -c buspirate -P COM3 -D -U flash:w:target/rf64.bin

# Onboard DFU programming with dfu-programmer:
param (
    [string]$HexPath = "target/rf64.hex"
)

if (-not (Test-Path $HexPath)) {
    Write-Error "Hex file not found at $HexPath. Run ./build_firmware.ps1 first!"
    exit 1
}

Write-Host "Erasing ATmega32U4 flash..." -ForegroundColor Yellow
dfu-programmer atmega32u4 erase

Write-Host "Flashing $HexPath..." -ForegroundColor Cyan
dfu-programmer atmega32u4 flash $HexPath

Write-Host "Starting application..." -ForegroundColor Green
dfu-programmer atmega32u4 start

Write-Host "Flashing Complete & Booted!" -ForegroundColor Green
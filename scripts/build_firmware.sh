#!/bin/bash
set -e

OUT_NAME="${OUT_NAME:-rf64}"

# Check if --release flag is passed
if [ "$1" = "--release" ]; then
    PROFILE_DIR="release"
    cargo build --release --target avr-none "${@:2}"
else
    PROFILE_DIR="debug"
    cargo build --target avr-none "${@:2}"
fi

# Use avr-objcopy to convert the ELF file to BIN and HEX files
cp "target/avr-none/$PROFILE_DIR/rf64.elf" "target/$OUT_NAME.elf"
avr-objcopy -O binary -R .eeprom -R .fuse -R .lock -R .signature -R .user_signatures -R .noinit -R .bss "target/$OUT_NAME.elf" "target/$OUT_NAME.bin"
avr-objcopy -O ihex -R .eeprom -R .fuse -R .lock -R .signature -R .user_signatures -R .noinit -R .bss "target/$OUT_NAME.elf" "target/$OUT_NAME.hex"

echo "Build successful. Output files:"
echo " - target/$OUT_NAME.elf"
echo " - target/$OUT_NAME.bin"
echo " - target/$OUT_NAME.hex"
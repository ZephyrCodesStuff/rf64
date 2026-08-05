#!/bin/bash
set -e

# Check if --release flag is passed
if [ "$1" = "--release" ]; then
    cargo build --release --target avr-none "${@:2}"
else
    cargo build --target avr-none "${@:2}"
fi

# Check if the build was successful
if [ $? -ne 0 ]; then
    echo "Build failed. Exiting."
    exit 1
fi

# Use avr-objcopy to convert the ELF file to BIN and HEX files
if [ "$1" = "--release" ]; then
    cp target/avr-none/release/rf64.elf target/rf64.elf
    avr-objcopy -O binary -R .eeprom -R .fuse -R .lock -R .signature -R .user_signatures -R .noinit -R .bss target/rf64.elf target/rf64.bin
    avr-objcopy -O ihex -R .eeprom -R .fuse -R .lock -R .signature -R .user_signatures -R .noinit -R .bss target/rf64.elf target/rf64.hex
else
    cp target/avr-none/debug/rf64.elf target/rf64.elf
    avr-objcopy -O binary -R .eeprom -R .fuse -R .lock -R .signature -R .user_signatures -R .noinit -R .bss target/rf64.elf target/rf64.bin
    avr-objcopy -O ihex -R .eeprom -R .fuse -R .lock -R .signature -R .user_signatures -R .noinit -R .bss target/rf64.elf target/rf64.hex
fi

echo "Build successful. Output files:"
echo " - target/rf64.elf"
echo " - target/rf64.bin"
echo " - target/rf64.hex"
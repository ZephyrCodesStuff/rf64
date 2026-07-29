#!/bin/sh
set -e

# Check if --release flag is passed
if [ "$1" = "--release" ]; then
    cargo build --release --target avr-none
else
    cargo build --target avr-none
fi

# Check if the build was successful
if [ $? -ne 0 ]; then
    echo "Build failed. Exiting."
    exit 1
fi

# Use avr-objcopy to convert the ELF file to a HEX file
if [ "$1" = "--release" ]; then
    avr-objcopy -O binary -R .eeprom -R .fuse -R .lock -R .signature -R .user_signatures -R .noinit -R .bss target/avr-none/release/rf64.elf target/rf64.bin
    avr-objcopy -O ihex -R .eeprom -R .fuse -R .lock -R .signature -R .user_signatures -R .noinit -R .bss target/avr-none/release/rf64.elf target/rf64.hex
else
    avr-objcopy -O binary -R .eeprom -R .fuse -R .lock -R .signature -R .user_signatures -R .noinit -R .bss target/avr-none/debug/rf64.elf target/rf64.bin
    avr-objcopy -O ihex -R .eeprom -R .fuse -R .lock -R .signature -R .user_signatures -R .noinit -R .bss target/avr-none/debug/rf64.elf target/rf64.hex
fi

echo "Build successful. Output files:"
if [ "$1" = "--release" ]; then
    echo " - target/rf64.bin"
    echo " - target/rf64.hex"
else
    echo " - target/rf64.bin"
    echo " - target/rf64.hex"
fi
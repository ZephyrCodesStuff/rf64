#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]

mod bootloader;
mod delay;
mod gpio;
mod keys;
mod led;

use atmega_hal::Peripherals;
use delay::delay_ms;
use gpio::LedPins;
use keys::{key_read_raw, key_setup};
use led::{Color, LedDriver, NUM_BUTTONS, PhysicalLedBuffer};

/// Set CPU prescaler to 1 (16 MHz full speed).
#[inline(always)]
fn cpu_init_16mhz() {
    unsafe {
        core::arch::asm!(
            "sts 0x61, {enable}",
            "sts 0x61, {div1}",
            enable = in(reg) 0x80u8, // CLKPCE (bit 7)
            div1   = in(reg) 0x00u8, // division factor 1 (16 MHz)
            options(nomem, nostack)
        );
    }
}

/// Disable JTAG on MCUCR to free PORTC/PORTF pins for GPIO.
#[inline(always)]
fn disable_jtag() {
    unsafe {
        core::arch::asm!(
            "sts 0x55, {jtd}",
            "sts 0x55, {jtd}",
            jtd = in(reg) 0x80u8, // JTD (bit 7)
            options(nomem, nostack)
        );
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[atmega_hal::entry]
fn main() -> ! {
    // -------------------------------------------------------------------------
    // 1. Hardware safeguards: WDT disable, bootloader check, 16 MHz CPU, JTAG disable
    // -------------------------------------------------------------------------
    bootloader::bootloader_jump_check();
    cpu_init_16mhz();
    disable_jtag();

    // -------------------------------------------------------------------------
    // 2. Initialize HAL peripherals, key matrix pins, & LED driver
    // -------------------------------------------------------------------------
    let dp = Peripherals::take().unwrap();
    let _led_pins = LedPins::init(&dp.PORTB, &dp.PORTC);
    let pins = atmega_hal::pins!(dp);

    // Disable interrupts during initialization to avoid race conditions
    avr_device::interrupt::disable();

    // Initialize key matrix pins using HAL pin abstractions
    key_setup(pins.pd7, pins.pd6, pins.pc7);

    // Check if Button 0 is held down at startup to jump into DFU bootloader
    let initial_keys = key_read_raw();
    if bootloader::bootloader_combo_held(initial_keys) {
        bootloader::jump_to_bootloader();
    }

    let led_driver = LedDriver::new();
    let mut buffer = PhysicalLedBuffer::new();

    // Colors per strand (all capped at <= 20% max brightness)
    let strand_colors = [
        Color::RED,   // Strand 0 (Buttons 0..15)
        Color::GREEN, // Strand 1 (Buttons 16..31)
        Color::CYAN,  // Strand 2 (Buttons 32..47)
        Color::WHITE, // Strand 3 (Buttons 48..63)
    ];

    // -------------------------------------------------------------------------
    // 3. Real-time button scanner loop
    // -------------------------------------------------------------------------
    loop {
        let pressed_keys = key_read_raw();

        buffer.clear();

        // Scan all 64 buttons
        for btn in 0..NUM_BUTTONS {
            if (pressed_keys & (1u64 << btn)) != 0 {
                let strand = btn / 16;
                buffer.set_button(btn, strand_colors[strand]);
            } else {
                buffer.set_button(btn, Color::new(2, 2, 2)); // Dim white for unpressed
            }
        }

        // Output current LED frame to hardware
        led_driver.update_display(&buffer);

        delay_ms(10);
    }
}

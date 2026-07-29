//! Key matrix reading for MIDI Fighter 64 matching exact C firmware (key.c).
//!
//! Pin Assignments on `ATmega32U4`:
//! - `KEY_CLOCK`: PD7 (PORTD bit 7)
//! - `KEY_LATCH`: PD6 (PORTD bit 6)
//! - `KEY_BIT`:   PC7 (PINC bit 7)

use core::arch::asm;

// ATmega32U4 I/O Space Register Addresses:
const PINC_IO: u8 = 0x06;
const PORTD_IO: u8 = 0x0B;

// Bit indexes
const CLOCK_BIT: u8 = 7; // PD7
const LATCH_BIT: u8 = 6; // PD6
const DATA_BIT: u8 = 7;  // PC7

use atmega_hal::port::mode::{Floating, Input};
use atmega_hal::port::{Pin, PC7, PD6, PD7};

/// Initialize key matrix shift register pins using safe HAL abstractions.
pub fn key_setup(
    clock: Pin<Input<Floating>, PD7>,
    latch: Pin<Input<Floating>, PD6>,
    data: Pin<Input<Floating>, PC7>,
) {
    let _ = clock.into_output_high();
    let _ = latch.into_output_high();
    let _ = data.into_pull_up_input();
}

/// Read all 64 keys immediately matching exact C firmware key.c algorithm.
/// Returns a bitmask where bit N = 1 means button N is currently pressed.
pub fn key_read_raw() -> u64 {
    unsafe {
        let mut value: u64 = 0;
        let mut bit: u64 = 1;

        // Shift latch pulse for CD4021BM: pulse HIGH then return LOW to shift
        asm!(
            "sbi {portd}, {latch}",
            "nop", "nop",
            "cbi {portd}, {latch}",
            portd = const PORTD_IO,
            latch = const LATCH_BIT,
            options(nomem, nostack)
        );

        // Shift 64 bits from shift registers
        for _ in 0..64 {
            // Clock falling edge
            asm!(
                "cbi {portd}, {clock}",
                "nop", "nop", "nop", "nop",
                portd = const PORTD_IO,
                clock = const CLOCK_BIT,
                options(nomem, nostack)
            );

            // Read PINC bit 7 (PC7). Active LOW: 0 = pressed
            let pinc: u8;
            asm!(
                "in {0}, {pinc}",
                out(reg) pinc,
                pinc = const PINC_IO,
                options(nomem, nostack)
            );

            if (pinc & (1 << DATA_BIT)) == 0 {
                value |= bit;
            }
            bit <<= 1;

            // Clock rising edge (shifts data on CD4021B)
            asm!(
                "nop", "nop", "nop", "nop",
                "sbi {portd}, {clock}",
                "nop", "nop", "nop", "nop",
                portd = const PORTD_IO,
                clock = const CLOCK_BIT,
                options(nomem, nostack)
            );
        }

        // Invert the bits: MF64 buttons are active LOW (0 = pressed).
        !value
    }
}

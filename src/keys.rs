//! Key matrix reading for MIDI Fighter 64 matching exact C firmware (key.c).
//!
//! Pin Assignments on ATmega32U4:
//! - KEY_CLOCK: PD7 (I/O address 0x0B, Bit 7)
//! - KEY_LATCH: PD6 (I/O address 0x0B, Bit 6)
//! - KEY_BIT:   PC7 (I/O address 0x06 / 0x08, Bit 7)

use core::arch::asm;

/// Initialize key matrix shift register pins.
/// Matches C key_setup() from official firmware.
pub fn key_setup() {
    unsafe {
        // DDRD is I/O 0x0A: set PD6 (LATCH) and PD7 (CLOCK) as outputs
        asm!("sbi 0x0a, 7", "sbi 0x0a, 6", options(nomem, nostack));

        // DDRC is I/O 0x07: set PC7 (DATA) as input
        // PORTC is I/O 0x08: enable pull-up resistor on PC7
        asm!("cbi 0x07, 7", "sbi 0x08, 7", options(nomem, nostack));

        // Leave CLOCK (PD7) and LATCH (PD6) HIGH in idle state
        asm!("sbi 0x0b, 7", "sbi 0x0b, 6", options(nomem, nostack));
    }
}

/// Read all 64 keys immediately matching exact C firmware key.c algorithm.
/// Returns a bitmask where bit N = 1 means button N is currently pressed.
pub fn key_read_raw() -> u64 {
    unsafe {
        let mut value: u64 = 0;
        let mut bit: u64 = 1;

        // Shift latch for CD4021BM: 
        // P/S pin is HIGH for Parallel Load, LOW for Serial Shift.
        // We pulse HIGH to load, then leave LOW to shift.
        asm!("sbi 0x0b, 6", options(nomem, nostack));
        asm!("nop", "nop", options(nomem, nostack));
        asm!("cbi 0x0b, 6", options(nomem, nostack));

        // Shift 64 bits from shift registers
        for _ in 0..64 {
            // Clock falling edge
            asm!("cbi 0x0b, 7", options(nomem, nostack));
            
            // Wait 250ns for the pin to settle
            asm!("nop", "nop", "nop", "nop", options(nomem, nostack));

            // Read PINC (I/O 0x06) bit 7 (PC7). Active LOW: 0 = pressed
            let pinc: u8;
            asm!("in {0}, 0x06", out(reg) pinc, options(nomem, nostack));

            if (pinc & (1 << 7)) == 0 {
                value |= bit;
            }
            bit <<= 1;
            
            // Wait 250ns before rising edge
            asm!("nop", "nop", "nop", "nop", options(nomem, nostack));

            // Clock rising edge (shifts data on CD4021B)
            asm!("sbi 0x0b, 7", options(nomem, nostack));
            
            // Wait 250ns for shift register internal propagation delay before looping
            asm!("nop", "nop", "nop", "nop", options(nomem, nostack));
        }

        // Invert the bits: MF64 buttons are active LOW (0 = pressed).
        // By bitwise-NOTting the value, 1 = pressed, 0 = unpressed.
        !value
    }
}

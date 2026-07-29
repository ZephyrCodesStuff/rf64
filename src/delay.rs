//! Delay utilities for 16 MHz ATmega32U4.

use core::arch::asm;

/// Busy-wait delay for a specified number of microseconds at 16 MHz.
#[inline(always)]
pub fn delay_us(mut us: u16) {
    while us > 0 {
        // At 16 MHz, 1 us = 16 clock cycles.
        // Loop decrement & branch overhead takes ~6 cycles, 10 NOPs = 16 cycles = 1.0 us.
        unsafe {
            asm!(
                "nop", "nop", "nop", "nop",
                "nop", "nop", "nop", "nop",
                "nop", "nop",
                options(nomem, nostack)
            );
        }
        us -= 1;
    }
}

/// Busy-wait delay for a specified number of milliseconds.
#[inline(always)]
pub fn delay_ms(ms: u16) {
    for _ in 0..ms {
        delay_us(1000);
    }
}

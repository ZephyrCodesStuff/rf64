//! Random number generation utilities for the ATmega32U4.
//!
//! Provides a 16-bit LCG for pseudo-random sequences and hardware entropy
//! seeding via Watchdog Timer oscillator jitter.

#[cfg(feature = "boot-anim")]
use atmega_hal::Peripherals;

/// Advance the LCG state by one step.
///
/// The constants `25173` and `13849` give a full-period cycle over all 65536
/// 16-bit values.
#[inline(always)]
pub const fn next_rand(s: u16) -> u16 {
    s.wrapping_mul(25173).wrapping_add(13849)
}

/// Obtain true hardware entropy from Watchdog Timer (WDT) oscillator jitter.
///
/// Samples the phase relationship between the 16 MHz CPU clock and the 128 kHz
/// WDT RC oscillator four times, mixing 4 bits of jitter per sample into a
/// 16-bit seed.
#[cfg(feature = "boot-anim")]
pub fn get_wdt_jitter_entropy(p: &Peripherals) -> u16 {
    let mut seed: u16 = 0;

    for _ in 0..4 {
        let mut count: u16 = 0;
        unsafe {
            // Enable WDT interrupt mode (~16ms period: WDP=0000)
            core::ptr::write_volatile(0x60 as *mut u8, (1 << 7) | (1 << 6)); // WDIF | WDIE

            // Count 16 MHz CPU cycles until the 128 kHz Watchdog RC oscillator ticks
            while (core::ptr::read_volatile(0x60 as *const u8) & (1 << 7)) == 0 {
                count = count.wrapping_add(1);
            }

            // Clear WDIF flag
            core::ptr::write_volatile(0x60 as *mut u8, 1 << 7);
        }

        // Mix 4 bits of phase jitter into the seed
        seed = (seed << 4) ^ (count & 0x0F);
    }

    // Disable Watchdog Timer using mandatory 2-step timed write sequence
    p.WDT.wdtcsr().write(|w| {
        w.wdce().set_bit();
        w.wde().set_bit();

        w
    });
    p.WDT.wdtcsr().write(|w| {
        w.wdce().clear_bit();
        w.wde().clear_bit();

        w
    });

    seed
}

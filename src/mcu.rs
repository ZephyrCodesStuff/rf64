//! Low-level MCU clock and peripheral initialization for ATmega32U4.

/// Obtains true hardware entropy from Watchdog Timer (WDT) oscillator jitter.
#[cfg(feature = "boot-anim")]
pub fn get_wdt_jitter_entropy() -> u16 {
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

    // Disable Watchdog Timer
    unsafe {
        core::ptr::write_volatile(0x60 as *mut u8, (1 << 4) | (1 << 3));
        core::ptr::write_volatile(0x60 as *mut u8, 0x00);
    }

    seed
}

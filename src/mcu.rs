//! Low-level MCU clock and peripheral initialization for ATmega32U4.

/// Set CPU prescaler to 1 (16 MHz full speed).
#[inline(always)]
pub fn cpu_init_16mhz() {
    unsafe {
        core::arch::asm!(
            "sts 0x61, {enable}",
            "sts 0x61, {div1}",
            enable = in(reg) 0b10000000_u8, // CLKPCE
            div1   = in(reg) 0_u8,          // division factor 1 (16 MHz)
            options(nomem, nostack)
        );
    }
}

/// Disable JTAG on MCUCR to free PORTC/PORTF pins for GPIO.
#[inline(always)]
pub fn disable_jtag() {
    unsafe {
        core::arch::asm!(
            "sts 0x55, {jtd}",
            "sts 0x55, {jtd}",
            jtd = in(reg) 0b10000000_u8, // JTD (bit 7)
            options(nomem, nostack)
        );
    }
}

/// Perform low-level hardware safeguards at boot.
pub fn init_hardware_safeguards() {
    crate::bootloader::bootloader_jump_check();
    cpu_init_16mhz();
    disable_jtag();
}

/// Harvest 16 bits of genuine hardware entropy by measuring phase jitter between
/// the 16 MHz main crystal clock and the independent internal 128 kHz WDT RC oscillator.
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

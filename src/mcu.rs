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

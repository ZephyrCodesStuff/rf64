//! LUFA DFU Bootloader entry for `ATmega32U4`.
//!
//! Matches the logic from `jumptoboot.c` in the original MF64 C firmware.

use atmega_hal::Peripherals;

use crate::delay::delay_ms;

/// The LUFA magic key value (must match jumptoboot.c: 0xDC42ACCA).
pub const MAGIC_BOOT_KEY: u32 = 0xDC42_ACCA;

/// LUFA DFU bootloader word address.
///
/// The right-shift by 1 is intentional, as AVR MCUs use 16-bit word-addressing.
const BOOTLOADER_WORD_ADDR: u16 = 0x7000 >> 1;

/// Static variable placed in `.noinit` section.
#[unsafe(link_section = ".noinit")]
static mut BOOT_KEY: u32 = 0;

/// Check at startup whether a bootloader jump was requested (and jump if so).
/// 
/// You should call this before any other initialization.
#[inline(always)]
pub fn check_bootloader_requested(p: &Peripherals) {
    let mcusr = p.CPU.mcusr().read();
    let wdt_was_reset = mcusr.wdrf().bit_is_set();
    let magic_boot_key = unsafe { core::ptr::read_volatile(&raw const BOOT_KEY) };

    // Always clear MCUSR and disable Watchdog timer on startup to prevent 16ms reset loop

    // Clear the MCU Status Registers
    p.CPU.mcusr().write(|w| {
        w.borf().clear_bit();
        w.extrf().clear_bit();
        w.jtrf().clear_bit();
        w.porf().clear_bit();
        w.wdrf().clear_bit();

        w
    });

    // Disable the Watchdog timer using the mandatory 2-step timed write sequence
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

    if wdt_was_reset && magic_boot_key == MAGIC_BOOT_KEY {
        unsafe {
            // Reset the boot key
            core::ptr::write_volatile(&raw mut BOOT_KEY, 0);

            // Jump to bootloader using IJMP via Z-register.
            core::arch::asm!(
                "movw r30, {addr}",
                "ijmp",
                addr = in(reg_iw) BOOTLOADER_WORD_ADDR,
                options(nomem, nostack, noreturn)
            );
        }
    }
}

/// Trigger a jump to the LUFA DFU bootloader via watchdog reset.
pub fn request_bootloader(p: &Peripherals) -> ! {
    avr_device::interrupt::disable();

    // 2-second pause to allow USB to cleanly detach from the host
    delay_ms(2000);

    // Write magic key to .noinit SRAM variable then trigger WDT reset
    unsafe {
        core::ptr::write_volatile(&raw mut BOOT_KEY, MAGIC_BOOT_KEY);

        // Open a 4-cycle window to modify the watchdog
        p.WDT.wdtcsr().write(|w| {
            w.wde().set_bit();
            w.wdce().set_bit();

            w
        });

        // Request a reset after 32k cycles (~250ms @ 16 MHz)
        p.WDT.wdtcsr().write(|w| {
            w.wde().set_bit();
            w.wdpl().cycles_32k();

            w
        });
    }

    #[allow(clippy::empty_loop, reason = "wait for WDT to reset the MCU")]
    loop {} // Spin until WDT fires
}

/// Returns `true` if button 0 is held at startup (used to trigger bootloader entry).
pub const fn bootloader_combo_held(key_state: crate::buttons::ButtonMask) -> bool {
    (key_state.0[0] & 0b1) != 0
}

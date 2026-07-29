//! LUFA DFU Bootloader entry for `ATmega32U4`.
//!
//! Matches the logic from `jumptoboot.c` in the original MF64 C firmware.

use crate::delay::delay_ms;

/// The LUFA magic key value (must match jumptoboot.c: 0xDC42ACCA).
pub const MAGIC_BOOT_KEY: u32 = 0xDC42_ACCA;

/// LUFA DFU bootloader word address (byte addr 0x7000 >> 1 = 0x3800).
const BOOTLOADER_WORD_ADDR: u16 = 0x3800;

/// Static variable placed in `.noinit` section.
#[unsafe(link_section = ".noinit")]
static mut BOOT_KEY: u32 = 0;

/// Read the MCU Status Register (MCUSR).
/// MCUSR is at I/O address 0x34 -> SRAM data address 0x54.
#[inline(always)]
fn read_mcusr() -> u8 {
    unsafe { core::ptr::read_volatile(0x54_u16 as *const u8) }
}

/// Clear MCUSR (important: must be done before disabling WDT or WDE cannot be cleared).
#[inline(always)]
fn clear_mcusr() {
    unsafe { core::ptr::write_volatile(0x54_u16 as *mut u8, 0x00) };
}

/// Disable the watchdog using the mandatory `ATmega32U4` timed write sequence.
unsafe fn wdt_disable() {
    unsafe {
        core::arch::asm!(
            "sts 0x60, {tmp}",   // WDTCSR is at SRAM 0x60 (I/O 0x40)
            "sts 0x60, {zero}",
            tmp  = in(reg) 0x18u8, // WDCE | WDE
            zero = in(reg) 0x00u8,
            options(nomem, nostack)
        );
    }
}

/// Enable watchdog with ~250ms timeout using the `ATmega32U4` timed write sequence.
unsafe fn wdt_enable_250ms() {
    unsafe {
        core::arch::asm!(
            "sts 0x60, {unlock}",
            "sts 0x60, {cfg}",
            unlock = in(reg) 0x18u8,       // WDCE | WDE
            cfg    = in(reg) 0x0Eu8,       // WDE | WDP2 | WDP1 = 250ms timeout
            options(nomem, nostack)
        );
    }
}

/// Check at startup whether a bootloader jump was requested.
/// Call this as the very first thing in `main()`, before any hardware init.
#[inline(always)]
pub fn bootloader_jump_check() {
    let mcusr = read_mcusr();
    // Bit 3 = WDRF (Watchdog Reset Flag)
    let was_wdt_reset = (mcusr & (1 << 3)) != 0;
    let key = unsafe { core::ptr::read_volatile(&raw const BOOT_KEY) };

    // Always clear MCUSR and disable Watchdog timer on startup to prevent 16ms reset loop
    clear_mcusr();
    unsafe {
        wdt_disable();
    }

    if was_wdt_reset && key == MAGIC_BOOT_KEY {
        unsafe {
            core::ptr::write_volatile(&raw mut BOOT_KEY, 0);
        }

        // Jump to bootloader using IJMP via Z-register.
        unsafe {
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
pub fn jump_to_bootloader() -> ! {
    avr_device::interrupt::disable();

    // 2-second pause to allow USB to cleanly detach from the host
    delay_ms(2000);

    // Write magic key to .noinit SRAM variable then trigger WDT reset
    unsafe {
        core::ptr::write_volatile(&raw mut BOOT_KEY, MAGIC_BOOT_KEY);
        wdt_enable_250ms();
    }

    #[allow(clippy::empty_loop, reason = "Wait for WDT to reset the MCU")]
    loop {} // Spin until WDT fires
}

/// Returns `true` if button 0 is held at startup (used to trigger bootloader entry).
pub const fn bootloader_combo_held(key_state: u64) -> bool {
    key_state & 0b1 == 0b1
}

//! Top-level USB device management for ATmega32U4.
//!
//! Owns the USB PLL initialization, bus reset, top-level device descriptors,
//! shared allocators, and polling dispatch between MIDI and Keyboard modes.

pub mod midi;

#[cfg(feature = "keyboard")]
pub mod keyboard;

use core::mem::MaybeUninit;
use usb_device::bus::UsbBusAllocator;
use usb_device::device::{UsbDevice, UsbDeviceBuilder, UsbVidPid};

/// Wrapper around `atmega_usbd::UsbBus<()>`
pub type TargetUsbBus = atmega_usbd::UsbBus<()>;

// ── Global USB storage ───────────────────────────────────────────────────────
// Placing these in module-level statics (BSS/data) removes them from main()'s
// stack frame. Two-phase init: write allocator first, then borrow it as
// 'static to construct UsbMidiStack.
//
// SAFETY: single-threaded AVR firmware — no concurrent access possible.
pub static mut BUS_ALLOC_STORAGE: MaybeUninit<UsbBusAllocator<TargetUsbBus>> =
    MaybeUninit::uninit();
pub static mut USB_DEV_STORAGE: MaybeUninit<UsbDevice<'static, TargetUsbBus>> =
    MaybeUninit::uninit();

/// DJTT Midi Fighter 64 USB Identifiers
pub const VID: u16 = 0x2580; // DJ TechTools
pub const PID: u16 = 0x0008; // Midi Fighter 64

static USB_META_MANUFACTURER: &str = "https://github.com/ZephyrCodesStuff/rf64";
#[cfg(not(feature = "mystrix"))]
static USB_META_PRODUCT: &str = "MIDI Fighter 67";
static USB_META_SERIAL: &str = "0xDEADBEEF";

/// Get it? Because the MIDI Fighter 64 is like as big as 5 of these combined?
#[cfg(feature = "mystrix")]
static USB_META_PRODUCT: &str = "Mystrix Pro Max";

pub fn reset_usb_bus() {
    unsafe {
        core::ptr::write_volatile(0xE0 as *mut u8, 1);
    }
    crate::delay::delay_ms(100);
    unsafe {
        core::ptr::write_volatile(0xE0 as *mut u8, 0);
    }
}

pub fn init_dev(alloc_ref: &'static UsbBusAllocator<TargetUsbBus>) {
    let dev = UsbDeviceBuilder::new(alloc_ref, UsbVidPid(VID, PID))
        .manufacturer(USB_META_MANUFACTURER)
        .product(USB_META_PRODUCT)
        .serial_number(USB_META_SERIAL)
        .device_class(0x00)
        .device_sub_class(0x00)
        .device_protocol(0x00)
        .max_power(480)
        .max_packet_size_0(64)
        .build();

    unsafe {
        let p = core::ptr::addr_of_mut!(USB_DEV_STORAGE);
        p.write(core::mem::MaybeUninit::new(dev));
    }
}

/// Enable the 48 MHz USB PLL from the 16 MHz crystal on `ATmega32U4`.
/// Matches `ATmega32U4` datasheet & LUFA `USB_OPT_AUTO_PLL` logic.
pub fn init_usb_pll() {
    unsafe {
        // PLLCSR register is at SRAM 0x49 (I/O 0x29)
        // Bit 4 = PINDIV (1 = 16 MHz crystal divided by 2 -> 8 MHz PLL input)
        // Bit 1 = PLLE (PLL Enable)
        // Bit 0 = PLOCK (PLL Lock Status)

        // Set PINDIV for 16MHz crystal and enable PLL
        core::ptr::write_volatile(0x49 as *mut u8, (1 << 4) | (1 << 1));

        // Wait until PLL achieves lock (PLOCK bit 0 set)
        while (core::ptr::read_volatile(0x49 as *const u8) & (1 << 0)) == 0 {}
    }
}

fn init_bus(usb: atmega_hal::pac::USB_DEVICE) -> &'static UsbBusAllocator<TargetUsbBus> {
    let alloc = atmega_usbd::UsbBus::new(usb);
    let alloc_ref = unsafe {
        let p = core::ptr::addr_of_mut!(BUS_ALLOC_STORAGE);
        p.write(MaybeUninit::new(alloc));
        (*p).assume_init_ref()
    };
    init_dev(alloc_ref);
    alloc_ref
}

/// Initialize the USB bus and MIDI stack into module-level static storage.
/// Call once from `main()` before any USB activity.
pub fn init_global(usb: atmega_hal::pac::USB_DEVICE) {
    let alloc_ref = init_bus(usb);
    midi::init_midi(alloc_ref);

    // Poll once to trigger bus.enable() so USBE=1 before force_reset
    let usb_dev = unsafe { (*core::ptr::addr_of_mut!(USB_DEV_STORAGE)).assume_init_mut() };
    let midi = unsafe { (*core::ptr::addr_of_mut!(midi::MIDI_STORAGE)).assume_init_mut() };
    usb_dev.poll(&mut [midi]);

    reset_usb_bus();
}

/// Initialize the USB bus and Keyboard stack into module-level static storage.
/// Call once from `main()` when booting into Keyboard emulation mode.
#[cfg(feature = "keyboard")]
pub fn init_keyboard_global(usb: atmega_hal::pac::USB_DEVICE) {
    unsafe {
        crate::usb::keyboard::usb::IS_KEYBOARD_MODE = true;
    }
    let alloc_ref = init_bus(usb);
    crate::usb::keyboard::usb::init(alloc_ref);

    let usb_dev = unsafe { (*core::ptr::addr_of_mut!(USB_DEV_STORAGE)).assume_init_mut() };
    let keyboard = unsafe {
        (*core::ptr::addr_of_mut!(crate::usb::keyboard::usb::KEYBOARD_STORAGE)).assume_init_mut()
    };
    usb_dev.poll(&mut [keyboard]);

    reset_usb_bus();
}

pub fn poll() -> bool {
    let usb_dev = unsafe { (*core::ptr::addr_of_mut!(USB_DEV_STORAGE)).assume_init_mut() };

    #[cfg(feature = "keyboard")]
    if unsafe { crate::usb::keyboard::usb::IS_KEYBOARD_MODE } {
        let keyboard = unsafe {
            (*core::ptr::addr_of_mut!(crate::usb::keyboard::usb::KEYBOARD_STORAGE))
                .assume_init_mut()
        };
        return usb_dev.poll(&mut [keyboard]);
    }

    let midi = unsafe { (*core::ptr::addr_of_mut!(midi::MIDI_STORAGE)).assume_init_mut() };
    usb_dev.poll(&mut [midi])
}

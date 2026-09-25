//! USB HID Keyboard class for the rf64 firmware.
//!
//! Owns the keyboard descriptor, the `KeyboardClass` USB class implementation,
//! global storage, and the send helper. Keyboard-mode init is in `usb.rs`
//! since it needs access to the private `init_dev` / `reset_usb_bus` helpers.

use core::mem::MaybeUninit;
use usb_device::bus::UsbBusAllocator;
use usb_device::class_prelude::*;

use crate::usb::TargetUsbBus;

/// Standard USB HID Boot Keyboard Report Descriptor.
pub const KEYBOARD_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x01, // USAGE_PAGE (Generic Desktop)
    0x09, 0x06, // USAGE (Keyboard)
    0xa1, 0x01, // COLLECTION (Application)
    0x05, 0x07, //   USAGE_PAGE (Keyboard)
    0x19, 0xe0, //   USAGE_MINIMUM (Keyboard LeftControl)
    0x29, 0xe7, //   USAGE_MAXIMUM (Keyboard Right GUI)
    0x15, 0x00, //   LOGICAL_MINIMUM (0)
    0x25, 0x01, //   LOGICAL_MAXIMUM (1)
    0x75, 0x01, //   REPORT_SIZE (1)
    0x95, 0x08, //   REPORT_COUNT (8)
    0x81, 0x02, //   INPUT (Data,Var,Abs)
    0x95, 0x01, //   REPORT_COUNT (1)
    0x75, 0x08, //   REPORT_SIZE (8)
    0x81, 0x03, //   INPUT (Cnst,Var,Abs)
    0x95, 0x05, //   REPORT_COUNT (5)
    0x75, 0x01, //   REPORT_SIZE (1)
    0x05, 0x08, //   USAGE_PAGE (LEDs)
    0x19, 0x01, //   USAGE_MINIMUM (Num Lock)
    0x29, 0x05, //   USAGE_MAXIMUM (Kana)
    0x91, 0x02, //   OUTPUT (Data,Var,Abs)
    0x95, 0x01, //   REPORT_COUNT (1)
    0x75, 0x03, //   REPORT_SIZE (3)
    0x91, 0x03, //   OUTPUT (Cnst,Var,Abs)
    0x95, 0x06, //   REPORT_COUNT (6)
    0x75, 0x08, //   REPORT_SIZE (8)
    0x15, 0x00, //   LOGICAL_MINIMUM (0)
    0x25, 0x65, //   LOGICAL_MAXIMUM (101)
    0x05, 0x07, //   USAGE_PAGE (Keyboard)
    0x19, 0x00, //   USAGE_MINIMUM (Reserved (no event indicated))
    0x29, 0x65, //   USAGE_MAXIMUM (Keyboard Application)
    0x81, 0x00, //   INPUT (Data,Ary,Abs)
    0xc0, // END_COLLECTION
];

/// USB HID Keyboard Class.
pub struct KeyboardClass<'a, B: UsbBus> {
    interface: InterfaceNumber,
    endpoint_in: EndpointIn<'a, B>,
}

impl<'a, B: UsbBus> KeyboardClass<'a, B> {
    pub fn new(alloc: &'a UsbBusAllocator<B>) -> Self {
        KeyboardClass {
            interface: alloc.interface(),
            endpoint_in: alloc.interrupt(8, 10),
        }
    }

    pub fn send_report(&self, report: &[u8; 8]) -> usb_device::Result<usize> {
        self.endpoint_in.write(report)
    }
}

impl<B: UsbBus> UsbClass<B> for KeyboardClass<'_, B> {
    fn get_configuration_descriptors(
        &self,
        writer: &mut DescriptorWriter,
    ) -> usb_device::Result<()> {
        writer.interface(
            self.interface,
            0x03, // HID Class
            0x01, // Boot Subclass
            0x01, // Keyboard Protocol
        )?;

        writer.write(
            0x21, // HID Descriptor
            &[
                0x11,
                0x01, // bcdHID 1.11
                0x00, // bCountryCode
                0x01, // bNumDescriptors
                0x22, // bDescriptorType (Report Descriptor)
                KEYBOARD_REPORT_DESCRIPTOR.len() as u8,
                0x00,
            ],
        )?;

        writer.endpoint(&self.endpoint_in)?;
        Ok(())
    }

    fn control_in(&mut self, xfer: ControlIn<B>) {
        let req = xfer.request();
        if req.request_type == usb_device::control::RequestType::Standard
            && req.recipient == usb_device::control::Recipient::Interface
            && req.request == usb_device::control::Request::GET_DESCRIPTOR
            && (req.value >> 8) as u8 == 0x22
        {
            xfer.accept_with(KEYBOARD_REPORT_DESCRIPTOR).ok();
        }
    }

    fn control_out(&mut self, xfer: ControlOut<B>) {
        let req = xfer.request();
        if req.request_type == usb_device::control::RequestType::Class
            && req.recipient == usb_device::control::Recipient::Interface
        {
            match req.request {
                0x0A => { xfer.accept().ok(); } // SET_IDLE
                0x0B => { xfer.accept().ok(); } // SET_PROTOCOL
                0x09 => { xfer.accept().ok(); } // SET_REPORT
                _ => {}
            }
        }
    }
}

// SAFETY: single-threaded AVR firmware — no concurrent access possible.
pub static mut KEYBOARD_STORAGE: MaybeUninit<KeyboardClass<'static, TargetUsbBus>> =
    MaybeUninit::uninit();

pub static mut IS_KEYBOARD_MODE: bool = false;

/// Initialize the [`KeyboardClass`] into [`KEYBOARD_STORAGE`].
/// Called from `usb::init_keyboard_global`.
pub fn init(alloc_ref: &'static UsbBusAllocator<TargetUsbBus>) {
    unsafe {
        let p = core::ptr::addr_of_mut!(KEYBOARD_STORAGE);
        p.write(MaybeUninit::new(KeyboardClass::new(alloc_ref)));
    }
}

/// Send an 8-byte HID keyboard report to the host.
pub fn send_report(report: &[u8; 8]) -> usb_device::Result<usize> {
    let keyboard = unsafe { (*core::ptr::addr_of_mut!(KEYBOARD_STORAGE)).assume_init_mut() };
    keyboard.send_report(report)
}

/// Processes active button states and generates an 8-byte USB HID Boot Keyboard report.
/// Separates modifier keys (0xE0..=0xE7) into byte 0 and regular keycodes into bytes 2..7.
/// Returns the report and a boolean indicating if the FN key is held.
pub fn build_keyboard_report(pressed_keys: u64) -> ([u8; 8], bool) {
    let mut report = [0u8; 8];
    let mut modifier = 0u8;
    let mut count = 0;

    // First pass: check for FN key
    let mut is_fn_pressed = false;
    crate::buttons::for_each_button(pressed_keys, |btn| {
        if super::layout::LAYER_0.load_at(btn as usize) == 0xFF {
            is_fn_pressed = true;
        }
    });

    // Second pass: build report from active layer
    crate::buttons::for_each_button(pressed_keys, |btn| {
        let key = if is_fn_pressed {
            super::layout::LAYER_1.load_at(btn as usize)
        } else {
            super::layout::LAYER_0.load_at(btn as usize)
        };

        // Skip unmapped keys or the FN key itself
        if key != 0x00 && key != 0xFF {
            if (0xE0..=0xE7).contains(&key) {
                modifier |= 1 << (key - 0xE0);
            } else if count < 6 {
                report[2 + count] = key;
                count += 1;
            }
        }
    });

    report[0] = modifier;
    (report, is_fn_pressed)
}

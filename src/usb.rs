use core::mem::MaybeUninit;
use usb_device::bus::UsbBusAllocator;
use usb_device::class_prelude::*;
use usb_device::device::{UsbDevice, UsbDeviceBuilder, UsbVidPid};

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
pub static mut MIDI_STORAGE: MaybeUninit<MidiClass<'static, TargetUsbBus>> = MaybeUninit::uninit();
#[cfg(feature = "keyboard")]
pub static mut KEYBOARD_STORAGE: MaybeUninit<KeyboardClass<'static, TargetUsbBus>> =
    MaybeUninit::uninit();
#[cfg(feature = "keyboard")]
pub static mut IS_KEYBOARD_MODE: bool = false;

fn reset_usb_bus() {
    unsafe {
        core::ptr::write_volatile(0xE0 as *mut u8, 1);
    }
    crate::delay::delay_ms(100);
    unsafe {
        core::ptr::write_volatile(0xE0 as *mut u8, 0);
    }
}

fn init_dev(alloc_ref: &'static UsbBusAllocator<TargetUsbBus>) {
    let dev = UsbDeviceBuilder::new(alloc_ref, UsbVidPid(VID, PID))
        .manufacturer("https://github.com/ZephyrCodesStuff/rf64")
        .product("Rusty Fighter 64")
        .serial_number(r"0xDEADBEEF")
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

/// Initialize the USB bus and MIDI stack into module-level static storage.
/// Call once from `main()` before any USB activity.
pub fn init_global(usb: atmega_hal::pac::USB_DEVICE) {
    let alloc = atmega_usbd::UsbBus::new(usb);
    let alloc_ref = unsafe {
        let p = core::ptr::addr_of_mut!(BUS_ALLOC_STORAGE);
        p.write(core::mem::MaybeUninit::new(alloc));
        (*p).assume_init_ref()
    };

    init_midi(alloc_ref);
    init_dev(alloc_ref);

    // Poll once to trigger bus.enable() so USBE=1 before force_reset
    let usb_dev = unsafe { (*core::ptr::addr_of_mut!(USB_DEV_STORAGE)).assume_init_mut() };
    let midi = unsafe { (*core::ptr::addr_of_mut!(MIDI_STORAGE)).assume_init_mut() };
    usb_dev.poll(&mut [midi]);

    reset_usb_bus();
}

fn init_midi(alloc_ref: &'static UsbBusAllocator<TargetUsbBus>) {
    unsafe {
        let p = core::ptr::addr_of_mut!(MIDI_STORAGE);
        p.write(core::mem::MaybeUninit::new(MidiClass::new(alloc_ref)));
    }
}

/// Initialize the USB bus and Keyboard stack into module-level static storage.
/// Call once from `main()` when booting into Keyboard emulation mode.
#[cfg(feature = "keyboard")]
pub fn init_keyboard_global(usb: atmega_hal::pac::USB_DEVICE) {
    unsafe {
        IS_KEYBOARD_MODE = true;
    }
    let alloc = atmega_usbd::UsbBus::new(usb);
    let alloc_ref = unsafe {
        let p = core::ptr::addr_of_mut!(BUS_ALLOC_STORAGE);
        p.write(core::mem::MaybeUninit::new(alloc));
        (*p).assume_init_ref()
    };

    init_keyboard(alloc_ref);
    init_dev(alloc_ref);

    let usb_dev = unsafe { (*core::ptr::addr_of_mut!(USB_DEV_STORAGE)).assume_init_mut() };
    let keyboard = unsafe { (*core::ptr::addr_of_mut!(KEYBOARD_STORAGE)).assume_init_mut() };
    usb_dev.poll(&mut [keyboard]);

    reset_usb_bus();
}

#[cfg(feature = "keyboard")]
fn init_keyboard(alloc_ref: &'static UsbBusAllocator<TargetUsbBus>) {
    unsafe {
        let p = core::ptr::addr_of_mut!(KEYBOARD_STORAGE);
        p.write(core::mem::MaybeUninit::new(KeyboardClass::new(alloc_ref)));
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

/// DJTT Midi Fighter 64 USB Identifiers
pub const VID: u16 = 0x2580; // DJ TechTools
pub const PID: u16 = 0x0008; // Midi Fighter 64

/// Bi-directional USB MIDI Class supporting both MIDI IN (send) and MIDI OUT (receive).
pub struct MidiClass<'a, B: UsbBus> {
    standard_ac: InterfaceNumber,
    standard_mc: InterfaceNumber,
    standard_bulkout: EndpointOut<'a, B>,
    standard_bulkin: EndpointIn<'a, B>,

    read_buf: [u8; 64],
    read_len: usize,
    read_pos: usize,
}

impl<'a, B: UsbBus> MidiClass<'a, B> {
    pub fn new(alloc: &'a UsbBusAllocator<B>) -> Self {
        MidiClass {
            standard_ac: alloc.interface(),
            standard_mc: alloc.interface(),
            standard_bulkout: alloc.bulk(64),
            standard_bulkin: alloc.bulk(64),
            read_buf: [0; 64],
            read_len: 0,
            read_pos: 0,
        }
    }

    pub fn send_raw_packet(&self, bytes: [u8; 4]) -> usb_device::Result<usize> {
        self.standard_bulkin.write(&bytes)
    }

    pub fn read_packet(&mut self) -> usb_device::Result<[u8; 4]> {
        if self.read_pos + 4 <= self.read_len {
            let mut buf = [0u8; 4];
            buf.copy_from_slice(&self.read_buf[self.read_pos..self.read_pos + 4]);
            self.read_pos += 4;
            return Ok(buf);
        }

        let bytes_read = self.standard_bulkout.read(&mut self.read_buf)?;

        if bytes_read >= 4 {
            self.read_len = bytes_read;
            self.read_pos = 4;
            let mut buf = [0u8; 4];
            buf.copy_from_slice(&self.read_buf[0..4]);
            Ok(buf)
        } else {
            self.read_len = 0;
            self.read_pos = 0;
            Err(usb_device::UsbError::WouldBlock)
        }
    }

    pub const fn unread_packet(&mut self) {
        if self.read_pos >= 4 {
            self.read_pos -= 4;
        }
    }
}

impl<B: UsbBus> UsbClass<B> for MidiClass<'_, B> {
    fn get_configuration_descriptors(
        &self,
        writer: &mut DescriptorWriter,
    ) -> usb_device::Result<()> {
        // Audio Control Standard Interface
        writer.interface(
            self.standard_ac,
            0x01, // USB_AUDIO_CLASS
            0x01, // USB_AUDIOCONTROL_SUBCLASS
            0,
        )?;

        // Audio Control Class-Specific Header
        writer.write(
            0x24, // CS_INTERFACE
            &[
                0x01, // HEADER_SUBTYPE
                0x00, 0x01, // REVISION
                0x09, 0x00, // SIZE
                0x01, // 1 streaming interface
                0x01, // MIDIStreaming interface 1
            ],
        )?;

        // MIDI Streaming Standard Interface
        writer.interface(
            self.standard_mc,
            0x01, // USB_AUDIO_CLASS
            0x03, // USB_MIDISTREAMING_SUBCLASS
            0,
        )?;

        // Class-Specific MS Header Descriptor (7 + 6 + 6 + 9 + 9 = 37 bytes. BUT MacOS wants Endpoints included so 65 bytes = 0x41)
        writer.write(
            0x24, // CS_INTERFACE
            &[
                0x01, // MS_HEADER_SUBTYPE
                0x00, 0x01, // REVISION (1.0)
                0x41, 0x00, // Total length LSB, MSB (65 bytes)
            ],
        )?;

        // 1. Embedded MIDI IN Jack (ID 0x01) — Receives data from Host via Bulk OUT Endpoint
        writer.write(
            0x24,
            &[
                0x02, // MIDI_IN_JACK_SUBTYPE (Input Terminal)
                0x01, // EMBEDDED
                0x01, // Jack ID
                0x00, // String Index
            ],
        )?;

        // 2. External MIDI IN Jack (ID 0x02) — Represents physical button/control input
        writer.write(
            0x24,
            &[
                0x02, // MIDI_IN_JACK_SUBTYPE (Input Terminal)
                0x02, // EXTERNAL
                0x02, // Jack ID
                0x00, // String Index
            ],
        )?;

        // 3. Embedded MIDI OUT Jack (ID 0x03) — Transmits data to Host via Bulk IN Endpoint (Source: External In 0x02)
        writer.write(
            0x24,
            &[
                0x03, // MIDI_OUT_JACK_SUBTYPE (Output Terminal)
                0x01, // EMBEDDED
                0x03, // Jack ID
                0x01, // 1 pin
                0x02, // Source Jack ID (External IN Jack 0x02)
                0x01, // Source Pin ID 1
                0x00, // String Index
            ],
        )?;

        // 4. External MIDI OUT Jack (ID 0x04) — Represents internal synth/LED destination (Source: Embedded In 0x01)
        writer.write(
            0x24,
            &[
                0x03, // MIDI_OUT_JACK_SUBTYPE (Output Terminal)
                0x02, // EXTERNAL
                0x04, // Jack ID
                0x01, // 1 pin
                0x01, // Source Jack ID (Embedded IN Jack 0x01)
                0x01, // Source Pin ID 1
                0x00, // String Index
            ],
        )?;

        // Bulk OUT Endpoint (Host -> Device)
        writer.endpoint_ex(&self.standard_bulkout, |buf| {
            buf[0] = 0; // bRefresh
            buf[1] = 0; // bSynchAddress
            Ok(2)
        })?;
        writer.write(
            0x25, // CS_ENDPOINT
            &[
                0x01, // MS_GENERAL
                0x01, // 1 embedded jack
                0x01, // Associated Jack ID (Embedded IN Jack 0x01)
            ],
        )?;

        // Bulk IN Endpoint (Device -> Host)
        writer.endpoint_ex(&self.standard_bulkin, |buf| {
            buf[0] = 0; // bRefresh
            buf[1] = 0; // bSynchAddress
            Ok(2)
        })?;
        writer.write(
            0x25, // CS_ENDPOINT
            &[
                0x01, // MS_GENERAL
                0x01, // 1 embedded jack
                0x03, // Associated Jack ID (Embedded OUT Jack 0x03)
            ],
        )?;

        Ok(())
    }
}

/// Standard 63-byte USB HID Boot Keyboard Report Descriptor
#[cfg(feature = "keyboard")]
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

/// USB HID Keyboard Class
#[cfg(feature = "keyboard")]
pub struct KeyboardClass<'a, B: UsbBus> {
    interface: InterfaceNumber,
    endpoint_in: EndpointIn<'a, B>,
}

#[cfg(feature = "keyboard")]
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

#[cfg(feature = "keyboard")]
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
                0x0A => {
                    xfer.accept().ok();
                } // SET_IDLE
                0x0B => {
                    xfer.accept().ok();
                } // SET_PROTOCOL
                0x09 => {
                    xfer.accept().ok();
                } // SET_REPORT
                _ => {}
            }
        }
    }
}

pub fn poll() -> bool {
    let usb_dev = unsafe { (*core::ptr::addr_of_mut!(USB_DEV_STORAGE)).assume_init_mut() };

    #[cfg(feature = "keyboard")]
    if unsafe { IS_KEYBOARD_MODE } {
        let keyboard = unsafe { (*core::ptr::addr_of_mut!(KEYBOARD_STORAGE)).assume_init_mut() };
        return usb_dev.poll(&mut [keyboard]);
    }

    let midi = unsafe { (*core::ptr::addr_of_mut!(MIDI_STORAGE)).assume_init_mut() };
    usb_dev.poll(&mut [midi])
}

pub fn read_packet() -> Option<[u8; 4]> {
    let midi = unsafe { (*core::ptr::addr_of_mut!(MIDI_STORAGE)).assume_init_mut() };
    midi.read_packet().ok()
}

pub fn unread_packet() {
    let midi = unsafe { (*core::ptr::addr_of_mut!(MIDI_STORAGE)).assume_init_mut() };
    midi.unread_packet();
}

pub fn send_raw_packet(bytes: [u8; 4]) -> usb_device::Result<usize> {
    let midi = unsafe { (*core::ptr::addr_of_mut!(MIDI_STORAGE)).assume_init_mut() };
    midi.send_raw_packet(bytes)
}

#[cfg(feature = "keyboard")]
pub fn send_keyboard_report(report: &[u8; 8]) -> usb_device::Result<usize> {
    let keyboard = unsafe { (*core::ptr::addr_of_mut!(KEYBOARD_STORAGE)).assume_init_mut() };
    keyboard.send_report(report)
}

use usb_device::bus::UsbBusAllocator;
use usb_device::class_prelude::*;
use usb_device::device::{UsbDevice, UsbDeviceBuilder, UsbVidPid};

pub type TargetUsbBus = atmega_usbd::UsbBus<()>;

/// Enable the 48 MHz USB PLL from the 16 MHz crystal on ATmega32U4.
/// Matches ATmega32U4 datasheet & LUFA USB_OPT_AUTO_PLL logic.
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

/// Create a UsbBusAllocator using ATmega32U4 USB_DEVICE peripheral.
pub fn create_usb_bus(usb: atmega_hal::pac::USB_DEVICE) -> UsbBusAllocator<TargetUsbBus> {
    atmega_usbd::UsbBus::new(usb)
}

/// Bi-directional USB MIDI Class supporting both MIDI IN (send) and MIDI OUT (receive).
pub struct MidiClass<'a, B: UsbBus> {
    standard_ac: InterfaceNumber,
    standard_mc: InterfaceNumber,
    standard_bulkout: EndpointOut<'a, B>,
    standard_bulkin: EndpointIn<'a, B>,
}

impl<'a, B: UsbBus> MidiClass<'a, B> {
    pub fn new(alloc: &'a UsbBusAllocator<B>) -> Self {
        MidiClass {
            standard_ac: alloc.interface(),
            standard_mc: alloc.interface(),
            standard_bulkout: alloc.bulk(64),
            standard_bulkin: alloc.bulk(64),
        }
    }

    pub fn send_message(
        &mut self,
        usb_midi: usbd_midi::data::usb_midi::usb_midi_event_packet::UsbMidiEventPacket,
    ) -> usb_device::Result<usize> {
        let bytes: [u8; 4] = usb_midi.into();
        self.standard_bulkin.write(&bytes)
    }

    pub fn read_packet(&mut self) -> usb_device::Result<[u8; 4]> {
        let mut buf = [0u8; 4];
        let bytes_read = self.standard_bulkout.read(&mut buf)?;
        if bytes_read >= 4 {
            Ok(buf)
        } else {
            Err(usb_device::UsbError::WouldBlock)
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

pub struct UsbMidiStack<'a> {
    pub usb_dev: UsbDevice<'a, TargetUsbBus>,
    pub midi: MidiClass<'a, TargetUsbBus>,
}

impl<'a> UsbMidiStack<'a> {
    pub fn new(bus: &'a UsbBusAllocator<TargetUsbBus>) -> Self {
        let mut midi = MidiClass::new(bus);

        let mut usb_dev = UsbDeviceBuilder::new(bus, UsbVidPid(VID, PID))
            .manufacturer("https://github.com/ZephyrCodesStuff/rf64")
            .product("Rusty Fighter 64")
            // .serial_number(r#"¯\_(ツ)_/¯"#)
            .serial_number(r#"0xDEADBEEF"#)
            .device_class(0x00)
            .device_sub_class(0x00)
            .device_protocol(0x00)
            .max_power(480)
            .build();

        // Poll once to trigger bus.enable() so USBE=1 before force_reset
        usb_dev.poll(&mut [&mut midi]);

        // Force a robust 100ms USB detach to ensure macOS recognizes the reset from bootloader
        unsafe {
            core::ptr::write_volatile(0xE0 as *mut u8, 1);
        }
        crate::delay::delay_ms(100);
        unsafe {
            core::ptr::write_volatile(0xE0 as *mut u8, 0);
        }

        UsbMidiStack { usb_dev, midi }
    }

    pub fn poll(&mut self) -> bool {
        self.usb_dev.poll(&mut [&mut self.midi])
    }

    pub fn read_packet(&mut self) -> Option<[u8; 4]> {
        self.midi.read_packet().ok()
    }
}

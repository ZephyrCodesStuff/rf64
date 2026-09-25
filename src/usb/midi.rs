//! USB MIDI class implementation for the MIDI Fighter 64.
//!
//! Provides bidirectional USB MIDI endpoints (Bulk IN/OUT) and SysEx packetization.

use core::mem::MaybeUninit;
use usb_device::bus::UsbBusAllocator;
use usb_device::class_prelude::*;
use usbd_midi::data::usb::constants::{
    CS_INTERFACE, EMBEDDED, HEADER_SUBTYPE, MIDI_OUT_JACK_SUBTYPE, MS_GENERAL, MS_HEADER_SUBTYPE,
    USB_AUDIO_CLASS, USB_AUDIOCONTROL_SUBCLASS, USB_MIDISTREAMING_SUBCLASS,
};

use crate::usb::TargetUsbBus;

// ── Global MIDI storage ───────────────────────────────────────────────────────
// SAFETY: single-threaded AVR firmware — no concurrent access possible.
pub static mut MIDI_STORAGE: MaybeUninit<MidiClass<'static, TargetUsbBus>> = MaybeUninit::uninit();

pub fn init_midi(alloc_ref: &'static UsbBusAllocator<TargetUsbBus>) {
    unsafe {
        let p = core::ptr::addr_of_mut!(MIDI_STORAGE);
        p.write(MaybeUninit::new(MidiClass::new(alloc_ref)));
    }
}

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
}

impl<B: UsbBus> UsbClass<B> for MidiClass<'_, B> {
    fn get_configuration_descriptors(
        &self,
        writer: &mut DescriptorWriter,
    ) -> usb_device::Result<()> {
        // Audio Control Standard Interface
        writer.interface(
            self.standard_ac,
            USB_AUDIO_CLASS,
            USB_AUDIOCONTROL_SUBCLASS,
            0,
        )?;

        // Audio Control Class-Specific Header
        writer.write(
            CS_INTERFACE,
            &[
                HEADER_SUBTYPE,
                0x00,
                0x01, // REVISION
                0x09,
                0x00, // SIZE
                0x01, // 1 streaming interface
                0x01, // MIDIStreaming interface 1
            ],
        )?;

        // MIDI Streaming Standard Interface
        writer.interface(
            self.standard_mc,
            USB_AUDIO_CLASS,
            USB_MIDISTREAMING_SUBCLASS,
            0,
        )?;

        // Class-Specific MS Header Descriptor (65 bytes total = 0x41)
        writer.write(
            CS_INTERFACE,
            &[
                MS_HEADER_SUBTYPE,
                0x00,
                0x01, // REVISION (1.0)
                0x41,
                0x00, // Total length LSB, MSB (65 bytes)
            ],
        )?;

        // 1. Embedded MIDI IN Jack (ID 0x01) — Receives data from Host via Bulk OUT Endpoint
        writer.write(
            CS_INTERFACE,
            &[
                0x02,     // MIDI_IN_JACK_SUBTYPE (Input Terminal)
                EMBEDDED, // EMBEDDED
                0x01,     // Jack ID
                0x00,     // String Index
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
            CS_INTERFACE,
            &[
                MIDI_OUT_JACK_SUBTYPE, // Output Terminal
                0x01,                  // EMBEDDED
                0x03,                  // Jack ID
                0x01,                  // 1 pin
                0x02,                  // Source Jack ID (External IN Jack 0x02)
                0x01,                  // Source Pin ID 1
                0x00,                  // String Index
            ],
        )?;

        // 4. External MIDI OUT Jack (ID 0x04) — Represents internal synth/LED destination (Source: Embedded In 0x01)
        writer.write(
            CS_INTERFACE,
            &[
                MIDI_OUT_JACK_SUBTYPE, // Output Terminal
                0x02,                  // EXTERNAL
                0x04,                  // Jack ID
                0x01,                  // 1 pin
                0x01,                  // Source Jack ID (Embedded IN Jack 0x01)
                0x01,                  // Source Pin ID 1
                0x00,                  // String Index
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
                MS_GENERAL,
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
                MS_GENERAL,
                0x01, // 1 embedded jack
                0x03, // Associated Jack ID (Embedded OUT Jack 0x03)
            ],
        )?;

        Ok(())
    }
}

pub fn read_packet() -> Option<[u8; 4]> {
    let midi = unsafe { (*core::ptr::addr_of_mut!(MIDI_STORAGE)).assume_init_mut() };
    midi.read_packet().ok()
}

pub fn send_raw_packet(bytes: [u8; 4]) -> usb_device::Result<usize> {
    let midi = unsafe { (*core::ptr::addr_of_mut!(MIDI_STORAGE)).assume_init_mut() };
    midi.send_raw_packet(bytes)
}

/// Send a complete SysEx payload over USB MIDI, handling multi-packet framing.
pub fn send_sysex(data: &[u8]) {
    let mut i = 0;
    while i < data.len() {
        let remaining = data.len() - i;
        let packet: [u8; 4] = if remaining >= 3 {
            if i == 0 {
                [0x4, data[i], data[i + 1], data[i + 2]]
            } else if remaining == 3 && data[i + 2] == 0xF7 {
                [0x7, data[i], data[i + 1], data[i + 2]]
            } else {
                [0x4, data[i], data[i + 1], data[i + 2]]
            }
        } else if remaining == 2 {
            [0x6, data[i], data[i + 1], 0]
        } else {
            [0x5, data[i], 0, 0]
        };

        while send_raw_packet(packet).is_err() {
            crate::usb::poll();
        }
        i += 3;
    }
}

//! SysEx Parser for high-speed commands and device identification.

use crate::fastled;
use crate::led::{Color, TOTAL_LEDS};

pub const MIDI_MAX_SYSEX: usize = 192;

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Begin,
    CheckMid,
    Invalid,
    NonRealtime,
    Djtt,
    List6F,
    Compressed5F,
}

pub struct SysExParser {
    state: State,
    buffer: [u8; MIDI_MAX_SYSEX],
    length: usize,
}

impl SysExParser {
    pub const fn new() -> Self {
        Self {
            state: State::Begin,
            buffer: [0; MIDI_MAX_SYSEX],
            length: 0,
        }
    }

    /// Process a raw 4-byte USB MIDI Event Packet containing SysEx data.
    /// Returns `true` if the LED state was modified and requires a redraw.
    pub fn process_packet(
        &mut self,
        packet: &[u8; 4],

        host_leds: &mut [Color; TOTAL_LEDS],
    ) -> bool {
        let cin = packet[0] & 0x0F;
        let mut modified = false;

        match cin {
            0x4 => {
                // SysEx Start or Continue (3 bytes)
                self.handle_3sc(packet, host_leds);
            }
            0x5 => {
                // SysEx End with 1 byte
                self.handle_end(packet, 1, host_leds, &mut modified);
            }
            0x6 => {
                // SysEx End with 2 bytes
                self.handle_end(packet, 2, host_leds, &mut modified);
            }
            0x7 => {
                // SysEx End with 3 bytes
                self.handle_end(packet, 3, host_leds, &mut modified);
            }
            _ => {}
        }

        modified
    }

    fn handle_3sc(&mut self, packet: &[u8; 4], _host_leds: &mut [Color; TOTAL_LEDS]) {
        if self.state == State::Begin {
            // Start of a new message
            self.length = 0;
            let d1 = packet[1];
            let d2 = packet[2];
            let d3 = packet[3];

            if d1 == 0xF0 && d2 == 0x7E && d3 == 0x7F {
                self.state = State::NonRealtime;
            } else if d1 == 0xF0 && d2 == 0x00 && d3 == 0x01 {
                self.state = State::CheckMid;
            } else if d1 == 0xF0 && d2 == 0x6F {
                self.state = State::List6F;
                self.push(d3);
            } else if d1 == 0xF0 && d2 == 0x5F {
                self.state = State::Compressed5F;
                self.push(d3);
            } else {
                self.state = State::Invalid;
            }
        } else if self.state == State::CheckMid {
            let d1 = packet[1];
            let d2 = packet[2];
            let d3 = packet[3];

            if d1 == 0x79 {
                // Manufacturer ID 0x0179
                self.state = State::Djtt;
                self.push(d2);
                self.push(d3);
            } else {
                self.state = State::Invalid;
            }
        } else if self.state != State::Invalid {
            self.push(packet[1]);
            self.push(packet[2]);
            self.push(packet[3]);
        }
    }

    fn handle_end(
        &mut self,
        packet: &[u8; 4],
        valid_bytes: usize,

        host_leds: &mut [Color; TOTAL_LEDS],
        modified: &mut bool,
    ) {
        if self.state == State::Begin {
            // Handled 1-byte end message without prior start, which could be F0 6E F7
            if valid_bytes == 3 && packet[1] == 0xF0 && packet[2] == 0x6E {
                fastled::fastrgb_clear(host_leds);
                *modified = true;
            }
            return;
        }

        if self.state == State::CheckMid {
            if valid_bytes >= 1 && packet[1] == 0x79 {
                self.state = State::Djtt;
                if valid_bytes >= 2 {
                    self.push(packet[2]);
                }
                if valid_bytes == 3 {
                    self.push(packet[3]);
                }
            } else {
                self.state = State::Invalid;
            }
        } else if self.state != State::Invalid {
            if valid_bytes >= 1 {
                self.push(packet[1]);
            }
            if valid_bytes >= 2 {
                self.push(packet[2]);
            }
            if valid_bytes == 3 {
                self.push(packet[3]);
            }
        }

        if self.state != State::Invalid {
            self.dispatch(host_leds, modified);
        }

        self.state = State::Begin;
        self.length = 0;
    }

    const fn push(&mut self, byte: u8) {
        if self.length < MIDI_MAX_SYSEX {
            self.buffer[self.length] = byte;
            self.length += 1;
        } else {
            self.state = State::Invalid;
        }
    }

    fn dispatch(&self, host_leds: &mut [Color; TOTAL_LEDS], modified: &mut bool) {
        let payload = &self.buffer[0..self.length.saturating_sub(1)]; // Exclude F7 if present in buffer

        match self.state {
            State::NonRealtime => {
                if payload.len() >= 2 && payload[0] == 0x06 && payload[1] == 0x01 {
                    // Device identify request, respond!
                    let response = [
                        0xF0, 0x7E, 0x7F, 0x06, 0x02, 0x00, 0x01,
                        0x79, // Manufacturer ID 0x00 0x01 0x79
                        0x06, 0x00, // Family
                        0x01, 0x00, // Model
                        0x30, 0x24, // Year MSB, LSB
                        0x03, // Month
                        0x20, // Day
                        0xF7, // End
                    ];
                    self.send_sysex(&response);
                }
            }
            State::List6F => {
                fastled::fastrgb_list(payload, host_leds);
                *modified = true;
            }
            State::Compressed5F => {
                fastled::fastrgb_decompress(payload, host_leds);
                *modified = true;
            }
            _ => {}
        }
    }

    fn send_sysex(&self, data: &[u8]) {
        let mut i = 0;
        let len = data.len();

        while i < len {
            let remain = len - i;
            let packet: [u8; 4] = if remain >= 3 {
                if i == 0 {
                    [0x4, data[i], data[i + 1], data[i + 2]] // SysEx Start or Continue
                } else if remain == 3 && data[i + 2] == 0xF7 {
                    [0x7, data[i], data[i + 1], data[i + 2]] // SysEx End with 3 bytes
                } else {
                    [0x4, data[i], data[i + 1], data[i + 2]] // SysEx Start or Continue
                }
            } else if remain == 2 {
                [0x6, data[i], data[i + 1], 0] // SysEx End with 2 bytes
            } else {
                [0x5, data[i], 0, 0] // SysEx End with 1 byte
            };

            while crate::usb::send_raw_packet(packet).is_err() {
                crate::usb::poll();
            }

            i += 3;
        }
    }
}

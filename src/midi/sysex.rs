//! SysEx Parser for high-speed commands and device identification.

use crate::led::{Color, TOTAL_LEDS};
use crate::midi::fastled;
#[cfg(feature = "mystrix")]
use crate::midi::mystrix::MystrixState;

pub const MIDI_MAX_SYSEX: usize = 192;

// ── Main parser state ─────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Begin,
    CheckMid,
    Invalid,
    NonRealtime,
    Djtt,
    List6F,
    Compressed5F,
    #[cfg(feature = "mystrix")]
    Mystrix(MystrixState),
}

// ── Parser ────────────────────────────────────────────────────────────────────

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
            0x4 => self.handle_3sc(packet),
            0x5 => self.handle_end(packet, 1, host_leds, &mut modified),
            0x6 => self.handle_end(packet, 2, host_leds, &mut modified),
            0x7 => self.handle_end(packet, 3, host_leds, &mut modified),
            _ => {}
        }

        modified
    }

    fn handle_3sc(&mut self, packet: &[u8; 4]) {
        let (d1, d2, d3) = (packet[1], packet[2], packet[3]);

        // Handle Mystrix-specific transitions first; returns true if consumed.
        // cfg_select! compiles this entire block away when the feature is off.
        let handled = cfg_select! {
            feature = "mystrix" => match (self.state, d1, d2, d3) {
                (State::Begin, 0xF0, 0x00, 0x02) => {
                    self.length = 0;
                    self.state = State::Mystrix(MystrixState::CheckManufacturer);
                    true
                }
                (State::Mystrix(MystrixState::CheckManufacturer), 0x03, 0x4D, 0x58) => {
                    self.state = State::Mystrix(MystrixState::Header);
                    true
                }
                (State::Mystrix(MystrixState::CheckManufacturer), _, _, _) => {
                    self.state = State::Invalid;
                    true
                }
                (State::Mystrix(MystrixState::Header), 0x5E, _, _) => {
                    self.push(d2);
                    self.push(d3);
                    self.state = State::Mystrix(MystrixState::Data);
                    true
                }
                (State::Mystrix(MystrixState::Header), _, _, _) => {
                    self.state = State::Invalid;
                    true
                }
                (State::Mystrix(MystrixState::Data), _, _, _) => {
                    self.push(d1);
                    self.push(d2);
                    self.push(d3);
                    true
                }
                _ => false,
            },
            _ => false,
        };

        if handled {
            return;
        }

        // Common transitions (shared by all builds)
        match (self.state, d1, d2, d3) {
            (State::Begin, _, _, _) => {
                self.length = 0;
                self.state = match (d1, d2, d3) {
                    (0xF0, 0x7E, 0x7F) => State::NonRealtime,
                    (0xF0, 0x00, 0x01) => State::CheckMid,
                    (0xF0, 0x6F, _) => {
                        self.push(d3);
                        State::List6F
                    }
                    (0xF0, 0x5F, _) => {
                        self.push(d3);
                        State::Compressed5F
                    }
                    _ => State::Invalid,
                };
            }
            (State::CheckMid, 0x79, _, _) => {
                self.push(d2);
                self.push(d3);
                self.state = State::Djtt;
            }
            (State::CheckMid, _, _, _) => self.state = State::Invalid,
            (s, _, _, _) if s != State::Invalid => {
                self.push(d1);
                self.push(d2);
                self.push(d3);
            }
            _ => {}
        }
    }

    fn handle_end(
        &mut self,
        packet: &[u8; 4],
        valid_bytes: usize,
        host_leds: &mut [Color; TOTAL_LEDS],
        modified: &mut bool,
    ) {
        // Single-packet shorthand: F0 6E F7 = clear all LEDs
        if self.state == State::Begin {
            if valid_bytes == 3 && packet[1] == 0xF0 && packet[2] == 0x6E {
                fastled::fastrgb_clear(host_leds);
                *modified = true;
            }
            return;
        }

        // Handle Mystrix-specific end transitions; returns true if consumed.
        let handled = cfg_select! {
            feature = "mystrix" => match self.state {
                State::Mystrix(MystrixState::Header) => {
                    self.state = if packet[1] == 0x5E {
                        if valid_bytes >= 2 {
                            self.push(packet[2]);
                        }
                        if valid_bytes == 3 {
                            self.push(packet[3]);
                        }
                        State::Mystrix(MystrixState::Data)
                    } else {
                        State::Invalid
                    };
                    true
                }
                _ => false,
            },
            _ => false,
        };

        if !handled {
            // Common end transitions
            match self.state {
                State::CheckMid => {
                    self.state = if packet[1] == 0x79 {
                        if valid_bytes >= 2 {
                            self.push(packet[2]);
                        }
                        if valid_bytes == 3 {
                            self.push(packet[3]);
                        }
                        State::Djtt
                    } else {
                        State::Invalid
                    };
                }
                s if s != State::Invalid => {
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
                _ => {}
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
        let payload = &self.buffer[0..self.length.saturating_sub(1)]; // Exclude trailing F7

        match self.state {
            State::NonRealtime => {
                // Device identity inquiry — respond with our spoofed identity
                if payload.len() >= 2 && payload[0] == 0x06 && payload[1] == 0x01 {
                    let response = cfg_select! {
                        feature = "mystrix" => [
                            0xF0, 0x7E, 0x7F, 0x06, 0x02, 0x00, 0x02, 0x03, 0x4D,
                            0x58, // Mystrix manufacturer ID
                            0x11, 0x01, 0x00, 0x00, 0x00, 0x01, 0xF7,
                        ],
                        _ => [
                            0xF0, 0x7E, 0x7F, 0x06, 0x02, 0x00, 0x01,
                            0x79, // DJTT manufacturer ID
                            0x06, 0x00, // Family
                            0x01, 0x00, // Model
                            0x30, 0x24, // Year MSB, LSB
                            0x03, // Month
                            0x20, // Day
                            0xF7,
                        ],
                    };
                    crate::usb::midi::send_sysex(&response);
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
            #[cfg(feature = "mystrix")]
            State::Mystrix(MystrixState::Data) => {
                crate::midi::mystrix::apply_leds(payload, host_leds, modified);
            }
            _ => {}
        }
    }
}

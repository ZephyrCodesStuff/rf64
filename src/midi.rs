//! MIDI message sending for MIDI Fighter 64.
//!
//! Note mapping matches the original C firmware (midi.c / constants.h):
//!   `MIDI_BASENOTE` = 36 (C2), button N → note 36 + N
//!   `MIDI_CHANNEL`  = 14 (0-indexed on the wire, shown as Ch.15 in DAWs)
//!   `MIDI_VELOCITY` = 74
//!
//! Debounce strategy (low-latency):
//!   - `NoteOn` fires immediately on the FIRST press edge (falling).
//!   - `NoteOff` fires immediately on the FIRST release edge (rising).
//!   - Subsequent transitions within the debounce window are ignored.

use usbd_midi::data::{
    byte::{from_traits::FromClamped, u7::U7},
    midi::{channel::Channel, message::Message, notes::Note},
    usb_midi::{cable_number::CableNumber, usb_midi_event_packet::UsbMidiEventPacket},
};

// ── Constants matching C firmware defaults ────────────────────────────────────

/// MIDI note for button 0 (C2 = 36). Button N → `MIDI_BASENOTE` + N.
pub const MIDI_BASENOTE: u8 = 36;

/// MIDI channel (0-indexed wire value). 14 = Channel 15 in DAW display.
const MIDI_CHANNEL: Channel = Channel::Channel15;

/// Default note-on velocity.
///
/// NOTE: The 2017 MF64 C firmware used 74, but Launchpads always send 127, so we do the same for consistency.
const MIDI_VELOCITY: u8 = 127;

/// How many `poll()` cycles a button must be stable before direction changes.
/// Since each main loop cycle with LED updates takes ~5ms, 2 cycles = ~10ms debounce.
const DEBOUNCE_CYCLES: u8 = 2;

// ── Debounce state ────────────────────────────────────────────────────────────

/// Per-button debounce state machine.
///
/// We use the "send-on-first-edge, suppress-until-stable" strategy:
///   1. On the very first detected edge (press OR release), emit the MIDI
///      message instantly for minimum latency.
///   2. Start the debounce counter.
///   3. Ignore further transitions until the counter expires (button stable).
pub struct ButtonState {
    /// Bitmask: `1` = button is considered PRESSED in the debounced state.
    confirmed: u64,
    /// Cycles remaining before the debounce window is open again (0 = ready).
    counter: [u8; 64],
}

impl ButtonState {
    pub const fn new() -> Self {
        Self {
            confirmed: 0,
            counter: [0; 64],
        }
    }
}

// ── Note number → usbd-midi Note ─────────────────────────────────────────────

/// Convert a raw MIDI note number (0-127) to the `usbd-midi` Note enum.
/// `Note` is `#[repr(u8)]` starting at C1m = 0, so we can transmute safely
/// as long as the value is ≤ 127 (which our range 36-99 always is).
#[inline(always)]
fn note_from_u8(n: u8) -> Note {
    // Safety: Note is repr(u8) with variants 0-127; n is always ≤ 99 here.
    unsafe { core::mem::transmute::<u8, Note>(n) }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Process the current raw button bitmask against the previous debounce state.
/// Sends NoteOn/NoteOff immediately on first edge, then locks out for
/// `DEBOUNCE_CYCLES` cycles to suppress bounce.
///
/// Call once per main-loop iteration, after `buttons_read_raw()`.
pub fn process_buttons(raw: u64, state: &mut ButtonState) {
    for btn in 0usize..64 {
        let pressed = (raw & (1u64 << btn)) != 0;

        if state.counter[btn] > 0 {
            // Still in debounce window — count down and ignore transitions.
            state.counter[btn] -= 1;
            continue;
        }

        // Debounce window open: check for a new edge.
        let was_confirmed = (state.confirmed & (1u64 << btn)) != 0;
        if pressed != was_confirmed {
            // First edge → fire immediately, then start debounce window.
            if pressed {
                state.confirmed |= 1u64 << btn;
            } else {
                state.confirmed &= !(1u64 << btn);
            }
            state.counter[btn] = DEBOUNCE_CYCLES;

            let note_num = MIDI_BASENOTE + btn as u8;

            // Retry a few times if the TX endpoint is busy (e.g. simultaneous
            // button releases filling the FIFO). Silently dropping NoteOffs
            // causes LEDs to stay lit in the host DAW.
            for _ in 0..4u8 {
                let packet = UsbMidiEventPacket {
                    cable_number: CableNumber::Cable0,
                    message: if pressed {
                        Message::NoteOn(
                            MIDI_CHANNEL,
                            note_from_u8(note_num),
                            U7::from_clamped(MIDI_VELOCITY),
                        )
                    } else {
                        Message::NoteOff(
                            MIDI_CHANNEL,
                            note_from_u8(note_num),
                            U7::from_clamped(MIDI_VELOCITY),
                        )
                    },
                };
                if crate::usb::send_raw_packet(packet.into()).is_ok() {
                    break;
                }
                crate::usb::poll(); // flush the TX endpoint and retry
            }
        }
    }
}

// ── MIDI Packet Receiver & Frame Synchronizer ─────────────────────────────────

/// Handles receiving incoming USB MIDI packets from the host DAW, frame boundary
/// detection, and mapping host note/velocity commands to LED grid colors.
pub struct MidiRx {
    _private: (),
}

impl MidiRx {
    pub const fn new() -> Self {
        Self { _private: () }
    }

    /// Drain incoming USB MIDI packets until the stream is stable (~300us idle gap)
    /// or a frame boundary is crossed.
    ///
    /// Updates `host_leds`, cancels `animating` if host data arrives, and returns
    /// `(dirty, activity)` tuple.
    pub fn drain_incoming_frame(
        &self,
        host_leds: &mut [crate::led::Color; crate::led::TOTAL_LEDS],
        animating: &mut bool,
        #[cfg(feature = "apollo")] mut sysex_parser_opt: Option<&mut crate::sysex::SysExParser>,
        #[cfg(not(feature = "apollo"))] _sysex_parser_opt: Option<&mut ()>,
    ) -> (bool, bool) {
        let mut idle_cycles = 0;
        let mut received_on = 0u64;
        let mut force_draw = false;
        let mut dirty = false;
        let mut activity = false;

        loop {
            crate::usb::poll();
            let mut read_any = false;

            while let Some(packet) = crate::usb::read_packet() {
                read_any = true;

                let status = packet[1];
                let note = packet[2];
                let velocity = packet[3];
                let channel = status & 0x0F;
                let cmd = status & 0xF0;
                let cin = packet[0] & 0x0F;

                // SysEx processing
                if (0x4..=0x7).contains(&cin) {
                    #[cfg(feature = "apollo")]
                    {
                        if let Some(sysex_parser) = sysex_parser_opt.as_deref_mut() {
                            // For CIN 5, 6, 7 (ends), process and trigger redraw if needed
                            let modified = sysex_parser.process_packet(&packet, host_leds);
                            if modified {
                                activity = true;
                                dirty = true;
                                if *animating {
                                    *animating = false; // Stop animation if host sends data
                                    host_leds.fill(crate::led::Color::BLACK);
                                }
                            }
                        }
                    }
                    continue; // Skip the standard Note/CC processing for SysEx
                }

                let is_on = (cmd == 0x90) && (velocity > 0);
                let is_off = (cmd == 0x80) || ((cmd == 0x90) && (velocity == 0));
                let is_cc = cmd == 0xB0;

                if is_on || is_off || is_cc {
                    activity = true;
                    if *animating {
                        *animating = false; // Stop animation if host sends data
                        host_leds.fill(crate::led::Color::BLACK);
                        dirty = true;
                    }
                }

                // Handle MIDI Panic / All Notes Off (CC 123) sent when playback stops.
                if is_cc && note == 123 {
                    for led in host_leds.iter_mut() {
                        *led = crate::led::Color::BLACK;
                    }
                    dirty = true;
                } else if (is_on || is_off) && (MIDI_BASENOTE..(MIDI_BASENOTE + 64)).contains(&note)
                {
                    let btn = (note - MIDI_BASENOTE) as usize;

                    // Frame boundary detection: if this button already received an ON
                    // in this burst, and now receives an OFF, we've crossed into the next frame!
                    if is_off && (received_on & (1 << btn)) != 0 {
                        crate::usb::unread_packet();
                        force_draw = true;
                        break;
                    }
                    if is_on {
                        received_on |= 1 << btn;
                    }

                    let color = if is_on {
                        crate::palette::ABLETON_COLORS.load_at(velocity as usize)
                    } else {
                        crate::led::Color::BLACK
                    };
                    let base_led = btn * 2;
                    match channel {
                        2 => {
                            host_leds[base_led] = color;
                            host_leds[base_led + 1] = color;
                        }
                        3 => {
                            host_leds[base_led] = color;
                        }
                        4 => {
                            host_leds[base_led + 1] = color;
                        }
                        _ => {}
                    }
                    dirty = true;
                }

                if force_draw {
                    break;
                }
            }

            if force_draw {
                break;
            }

            if read_any {
                idle_cycles = 0; // reset idle counter if we got data
            } else {
                idle_cycles += 1;
                if idle_cycles > 30 {
                    break; // ~300us of idle time, stream is stable
                }
                crate::delay::delay_us(10);
            }
        }

        (dirty, activity)
    }
}

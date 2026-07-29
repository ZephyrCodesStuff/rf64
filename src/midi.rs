//! MIDI message sending for MIDI Fighter 64.
//!
//! Note mapping matches the original C firmware (midi.c / constants.h):
//!   MIDI_BASENOTE = 36 (C2), button N → note 36 + N
//!   MIDI_CHANNEL  = 14 (0-indexed on the wire, shown as Ch.15 in DAWs)
//!   MIDI_VELOCITY = 74
//!
//! Debounce strategy (low-latency):
//!   - NoteOn fires immediately on the FIRST press edge (falling).
//!   - NoteOff fires immediately on the FIRST release edge (rising).
//!   - Subsequent transitions within the debounce window are ignored.

use usbd_midi::data::{
    byte::{from_traits::FromClamped, u7::U7},
    midi::{channel::Channel, message::Message, notes::Note},
    usb_midi::{cable_number::CableNumber, usb_midi_event_packet::UsbMidiEventPacket},
};

use crate::usb::UsbMidiStack;

// ── Constants matching C firmware defaults ────────────────────────────────────

/// MIDI note for button 0 (C2 = 36). Button N → MIDI_BASENOTE + N.
pub const MIDI_BASENOTE: u8 = 36;

/// MIDI channel (0-indexed wire value). 14 = Channel 15 in DAW display.
const MIDI_CHANNEL: Channel = Channel::Channel15;

/// Default note-on velocity, matches C firmware G_EE_MIDI_VELOCITY = 74.
const MIDI_VELOCITY: u8 = 74;

/// How many poll() cycles a button must be stable before direction changes.
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
    /// `true` = button is considered PRESSED in the debounced state.
    confirmed: [bool; 64],
    /// Cycles remaining before the debounce window is open again (0 = ready).
    counter: [u8; 64],
}

impl ButtonState {
    pub const fn new() -> Self {
        ButtonState {
            confirmed: [false; 64],
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

/// Process the current raw key bitmask against the previous debounce state.
/// Sends NoteOn/NoteOff immediately on first edge, then locks out for
/// `DEBOUNCE_CYCLES` cycles to suppress bounce.
///
/// Call once per main-loop iteration, after `key_read_raw()`.
pub fn process_keys(raw: u64, state: &mut ButtonState, usb: &mut UsbMidiStack) {
    for btn in 0usize..64 {
        let pressed = (raw & (1u64 << btn)) != 0;

        if state.counter[btn] > 0 {
            // Still in debounce window — count down and ignore transitions.
            state.counter[btn] -= 1;
            continue;
        }

        // Debounce window open: check for a new edge.
        if pressed != state.confirmed[btn] {
            // First edge → fire immediately, then start debounce window.
            state.confirmed[btn] = pressed;
            state.counter[btn] = DEBOUNCE_CYCLES;

            let note = note_from_u8(MIDI_BASENOTE + btn as u8);
            let vel = U7::from_clamped(MIDI_VELOCITY);
            let message = if pressed {
                Message::NoteOn(MIDI_CHANNEL, note, vel)
            } else {
                Message::NoteOff(MIDI_CHANNEL, note, vel)
            };

            let packet = UsbMidiEventPacket {
                cable_number: CableNumber::Cable0,
                message,
            };

            // Ignore send errors (e.g. endpoint not yet ready during enumeration)
            usb.midi.send_message(packet).ok();
        }
    }
}

/// Process all incoming USB-MIDI packets received from the computer (Host -> Device).
///
/// Channel mapping matching official firmware (1-indexed DAWs):
///   - Channel 3 (wire ch 2): Controls BOTH LEDs on the target button.
///   - Channel 4 (wire ch 3): Controls the FIRST LED (led0) on the target button.
///   - Channel 5 (wire ch 4): Controls the SECOND LED (led1) on the target button.
///
/// Note number: `MIDI_BASENOTE + btn` (36..99). NoteOn (vel > 0) turns LED(s) ON;
/// NoteOff (or NoteOn vel == 0) turns LED(s) OFF.
pub fn process_incoming_midi(
    usb: &mut UsbMidiStack,
    host_leds: &mut [crate::led::Color; crate::led::TOTAL_LEDS],
    strand_colors: &[crate::led::Color; 4],
) {
    while let Some(packet) = usb.read_packet() {
        let status = packet[1];
        let note = packet[2];
        let velocity = packet[3];

        let channel = status & 0x0F;
        let cmd = status & 0xF0;

        let is_on = (cmd == 0x90) && (velocity > 0);
        let is_off = (cmd == 0x80) || ((cmd == 0x90) && (velocity == 0));

        if is_on || is_off {
            if note >= MIDI_BASENOTE && note < (MIDI_BASENOTE + 64) {
                let btn = (note - MIDI_BASENOTE) as usize;
                let strand = btn / 16;
                let color = if is_on {
                    strand_colors[strand]
                } else {
                    crate::led::Color::BLACK
                };
                let base_led = btn * 2;

                match channel {
                    2 => {
                        // Channel 3 (1-indexed): Both LEDs
                        host_leds[base_led] = color;
                        host_leds[base_led + 1] = color;
                    }
                    3 => {
                        // Channel 4 (1-indexed): First LED only
                        host_leds[base_led] = color;
                    }
                    4 => {
                        // Channel 5 (1-indexed): Second LED only
                        host_leds[base_led + 1] = color;
                    }
                    _ => {}
                }
            }
        }
    }
}

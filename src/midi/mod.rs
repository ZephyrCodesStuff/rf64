//! MIDI protocol, SysEx parsing, and LED feedback for the MIDI Fighter 64.

pub mod midi;
pub mod palette;

#[cfg(feature = "apollo")]
pub mod sysex;

#[cfg(feature = "apollo")]
pub mod fastled;

#[cfg(feature = "mystrix")]
pub mod mystrix;

pub use midi::{MidiRx, send_button_events};

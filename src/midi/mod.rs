//! MIDI protocol, SysEx parsing, and LED feedback for the MIDI Fighter 64.

pub mod io;
pub mod palette;

#[cfg(feature = "apollo")]
pub mod sysex;

#[cfg(feature = "apollo")]
pub mod fastled;

#[cfg(feature = "mystrix")]
pub mod mystrix;

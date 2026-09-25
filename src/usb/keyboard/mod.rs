//! USB HID Keyboard emulation mode for the MIDI Fighter 64.

pub mod layout;
pub mod usb;

pub use layout::get_button_color;
pub use usb::{build_keyboard_report, send_report};

/// Returns `true` if button 2 (3rd button, bit 2) is held at startup (triggers Keyboard mode).
pub const fn keyboard_combo_held(key_state: u64) -> bool {
    (key_state & 0b100) != 0
}

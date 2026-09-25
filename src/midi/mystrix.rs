//! Mystrix-specific SysEx LED protocol handling.
//!
//! The Mystrix uses a 4-byte `[idx, r, g, b]` format where `idx` is either
//! an XY address (`11..=88`) or a flat button index (`0..63`).

use crate::buttons::cell_to_btn;
use crate::led::{set_button_color, Color, TOTAL_LEDS};

/// Sub-state for parsing an in-flight Mystrix SysEx message.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MystrixState {
    /// Received `F0 00 02` — waiting for `03 4D 58` (manufacturer ID).
    CheckManufacturer,
    /// Received `03 4D 58` — waiting for `5E` (command byte).
    Header,
    /// Streaming 4-byte `[idx, r, g, b]` LED chunks.
    Data,
}

/// Apply a decoded Mystrix LED data payload to the host LED buffer.
///
/// Each 4-byte chunk is `[idx, r6, g6, b6]` where the color components are
/// 6-bit (0–63). `idx` is either an XY address (`11..=88`) or a flat button
/// index (`0..63`), both of which are remapped to MF64 physical buttons.
pub fn apply_leds(payload: &[u8], host_leds: &mut [Color; TOTAL_LEDS], modified: &mut bool) {
    for chunk in payload.as_chunks::<4>().0 {
        let idx = chunk[0] as usize;
        let r6 = chunk[1] & 0x3F;
        let g6 = chunk[2] & 0x3F;
        let b6 = chunk[3] & 0x3F;

        // Mystrix XY index (11..88) → MF64 physical button (0..63)
        let btn_opt = if (11..=88).contains(&idx) {
            let x = (idx % 10) as u8;
            let y = (idx / 10) as u8;
            if (1..=8).contains(&x) && (1..=8).contains(&y) {
                Some(cell_to_btn(y - 1, x - 1))
            } else {
                None
            }
        } else if idx < 64 {
            Some(cell_to_btn((idx / 8) as u8, (idx % 8) as u8))
        } else {
            None
        };

        if let Some(btn) = btn_opt
            && btn < 64
        {
            set_button_color(host_leds, btn, Color::from_rgb6(r6, g6, b6));
            *modified = true;
        }
    }
}

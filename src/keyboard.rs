//! Ergonomic 8x8 QWERTY Keyboard layout, color scheme, and report builder for MIDI Fighter 64.

use avr_progmem::progmem;
use crate::led::Color;

progmem! {
    /// Base Layer (Layer 0) - Standard Typing
    pub static progmem LAYER_0: [u8; 64] = [
        // ── Left Half (Cols 0..3), Bottom to Top ──────────────────────────
        // Btn 0..3 (Visual Row 7): FN, MUTE(0), VOLD(0), VOLU(0)
        0xFF, 0x00, 0x00, 0x00,
        // Btn 4..7 (Visual Row 6): LCTRL, LALT, LCMD, SPC
        0xE0, 0xE2, 0xE3, 0x2C,
        // Btn 8..11 (Visual Row 5): LSHFT, ;, ', ,
        0xE1, 0x33, 0x34, 0x36,
        // Btn 12..15 (Visual Row 4): Z, X, C, V
        0x1D, 0x1B, 0x06, 0x19,
        // Btn 16..19 (Visual Row 3): A, S, D, F
        0x04, 0x16, 0x07, 0x09,
        // Btn 20..23 (Visual Row 2): Q, W, E, R
        0x14, 0x1A, 0x08, 0x15,
        // Btn 24..27 (Visual Row 1): HOME, PGDN, PGUP, END
        0x4A, 0x4E, 0x4B, 0x4D,
        // Btn 28..31 (Visual Row 0 - Top): ESC, TAB, `, -
        0x29, 0x2B, 0x35, 0x2D,

        // ── Right Half (Cols 4..7), Bottom to Top ─────────────────────────
        // Btn 32..35 (Visual Row 7): PLAY(0), PREV(0), NEXT(0), FN
        0x00, 0x00, 0x00, 0xFF,
        // Btn 36..39 (Visual Row 6): SPC, LEFT, DOWN, RGHT
        0x2C, 0x50, 0x51, 0x4F,
        // Btn 40..43 (Visual Row 5): ., /, UP, ENTR
        0x37, 0x38, 0x52, 0x28,
        // Btn 44..47 (Visual Row 4): B, N, M, L
        0x05, 0x11, 0x10, 0x0F,
        // Btn 48..51 (Visual Row 3): G, H, J, K
        0x0A, 0x0B, 0x0D, 0x0E,
        // Btn 52..55 (Visual Row 2): T, Y, U, I
        0x17, 0x1C, 0x18, 0x0C,
        // Btn 56..59 (Visual Row 1): [, ], O, P
        0x2F, 0x30, 0x12, 0x13,
        // Btn 60..63 (Visual Row 0 - Top): =, \, DEL, BKSP
        0x2E, 0x31, 0x4C, 0x2A,
    ];

    /// Function Layer (Layer 1) - Numbers and F-Keys
    pub static progmem LAYER_1: [u8; 64] = [
        // ── Left Half (Cols 0..3), Bottom to Top ──────────────────────────
        // Btn 0..3 (Visual Row 7): FN, none, none, none
        0xFF, 0x00, 0x00, 0x00,
        // Btn 4..7 (Visual Row 6): none, none, none, none
        0x00, 0x00, 0x00, 0x00,
        // Btn 8..11 (Visual Row 5): none, none, none, none
        0x00, 0x00, 0x00, 0x00,
        // Btn 12..15 (Visual Row 4): none, none, none, none
        0x00, 0x00, 0x00, 0x00,
        // Btn 16..19 (Visual Row 3): 9, 0, none, none
        0x26, 0x27, 0x00, 0x00,
        // Btn 20..23 (Visual Row 2): 1, 2, 3, 4
        0x1E, 0x1F, 0x20, 0x21,
        // Btn 24..27 (Visual Row 1): F9, F10, F11, F12
        0x42, 0x43, 0x44, 0x45,
        // Btn 28..31 (Visual Row 0 - Top): F1, F2, F3, F4
        0x3A, 0x3B, 0x3C, 0x3D,

        // ── Right Half (Cols 4..7), Bottom to Top ─────────────────────────
        // Btn 32..35 (Visual Row 7): none, none, none, FN
        0x00, 0x00, 0x00, 0xFF,
        // Btn 36..39 (Visual Row 6): none, HOME, PGDN, END
        0x00, 0x4A, 0x4E, 0x4D,
        // Btn 40..43 (Visual Row 5): none, none, PGUP, none
        0x00, 0x00, 0x4B, 0x00,
        // Btn 44..47 (Visual Row 4): none, none, none, none
        0x00, 0x00, 0x00, 0x00,
        // Btn 48..51 (Visual Row 3): none, none, none, none
        0x00, 0x00, 0x00, 0x00,
        // Btn 52..55 (Visual Row 2): 5, 6, 7, 8
        0x22, 0x23, 0x24, 0x25,
        // Btn 56..59 (Visual Row 1): none, none, none, none
        0x00, 0x00, 0x00, 0x00,
        // Btn 60..63 (Visual Row 0 - Top): F5, F6, F7, F8
        0x3E, 0x3F, 0x40, 0x41,
    ];
}

/// Returns the base RGB color for a given HID key code.
pub const fn get_key_color(key: u8) -> Color {
    match key {
        // None/Empty
        0x00 => Color::new(0, 0, 0),
        // FN Key: White
        0xFF => Color::new(255, 255, 255),
        // Modifiers (LCTRL, LSHFT, LALT, LGUI): Magenta / Purple
        0xE0..=0xE7 => Color::new(220, 0, 220),
        // Numbers ('1'..'9', '0') and F-keys (F1..F12): Cyan
        0x1E..=0x27 | 0x3A..=0x45 => Color::new(0, 180, 255),
        // Letters ('a'..'z'): Green
        0x04..=0x1D => Color::new(0, 210, 30),
        // Navigation (DEL, UP, HOME, PGUP, END, LEFT, DOWN, RIGHT, PGDN): Amber / Yellow
        0x4A..=0x52 => Color::new(230, 160, 0),
        // Special Controls & Symbols (ESC, TAB, CAPS, BKSP, ENTR, SPC, etc.): Orange
        _ => Color::new(255, 80, 0),
    }
}

/// Returns the RGB color for a physical button (0..63) based on its functional category and active layer.
pub fn get_button_color(btn: usize, pressed: bool, is_fn_pressed: bool) -> Color {
    let key = if is_fn_pressed {
        LAYER_1.load_at(btn)
    } else {
        LAYER_0.load_at(btn)
    };

    let base = get_key_color(key);

    if key == 0x00 {
        return Color::new(0, 0, 0); // Don't illuminate unmapped keys
    }

    if pressed {
        base
    } else {
        base.scale_brightness(25)
    }
}

/// Processes active button states and generates an 8-byte USB HID Boot Keyboard report.
/// Separates modifier keys (0xE0..=0xE7) into byte 0 and regular keycodes into bytes 2..7.
/// Returns the report and a boolean indicating if the FN key is held.
pub fn build_keyboard_report(pressed_keys: u64) -> ([u8; 8], bool) {
    let mut report = [0u8; 8];
    let mut modifier = 0u8;
    let mut count = 0;

    // First pass: check for FN key
    let mut is_fn_pressed = false;
    for btn in 0..64 {
        if (pressed_keys & (1u64 << btn)) != 0
            && LAYER_0.load_at(btn) == 0xFF {
                is_fn_pressed = true;
                break;
            }
    }

    // Second pass: build report from active layer
    for btn in 0..64 {
        if (pressed_keys & (1u64 << btn)) != 0 {
            let key = if is_fn_pressed {
                LAYER_1.load_at(btn)
            } else {
                LAYER_0.load_at(btn)
            };

            // Skip unmapped keys or the FN key itself
            if key == 0x00 || key == 0xFF {
                continue;
            }

            if (0xE0..=0xE7).contains(&key) {
                modifier |= 1 << (key - 0xE0);
            } else if count < 6 {
                report[2 + count] = key;
                count += 1;
            }
        }
    }

    report[0] = modifier;
    (report, is_fn_pressed)
}

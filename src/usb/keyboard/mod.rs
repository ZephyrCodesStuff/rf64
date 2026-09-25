//! USB HID Keyboard emulation mode for the MIDI Fighter 64.

pub mod layout;
pub mod usb;

pub use layout::get_button_color;
pub use usb::{build_keyboard_report, send_report};

use atmega_hal::pac::TC1;
use crate::buttons::{ButtonMask, ButtonMatrix, Debouncer};
use crate::led::{self, Color, ParallelBitBuffer, TOTAL_LEDS};

/// Returns `true` if button 2 (3rd button, bit 2) is held at startup (triggers Keyboard mode).
pub const fn keyboard_combo_held(key_state: ButtonMask) -> bool {
    (key_state.0[0] & 0b100) != 0
}

/// Run the dedicated USB HID Keyboard event loop (never returns).
pub fn run_keyboard_mode(
    tc1: &TC1,
    button_matrix: &mut ButtonMatrix,
    debouncer: &mut Debouncer,
    host_leds: &mut [Color; TOTAL_LEDS],
    par_buf: &mut ParallelBitBuffer,
) -> ! {
    let mut prev_fn_pressed = false;

    // Render initial category background colors for all buttons (~10% brightness)
    for btn in 0..64 {
        let color = get_button_color(btn, false, false);
        led::set_button_color(host_leds, btn, color);
    }
    led::render_frame(par_buf, host_leds);

    let mut prev_report = [0u8; 8];

    loop {
        crate::usb::poll();

        let now_tick = tc1.tcnt1().read().bits();
        let raw_buttons = button_matrix.read_raw();
        let _ = debouncer.update(raw_buttons, now_tick);

        let (report, is_fn_pressed) = build_keyboard_report(debouncer.state);

        let fn_changed = is_fn_pressed != prev_fn_pressed;
        let report_changed = report != prev_report;

        if report_changed {
            let _ = send_report(&report);
            prev_report = report;
        }

        // Update LEDs if the HID report changed OR if the FN layer toggled
        if report_changed || fn_changed {
            prev_fn_pressed = is_fn_pressed;

            // Full category color when pressed, dim category color when unpressed
            // Colors change dynamically based on active layer!
            for btn in 0..64 {
                let is_pressed = debouncer.state.is_set(btn);
                let color = get_button_color(btn, is_pressed, is_fn_pressed);
                led::set_button_color(host_leds, btn, color);
            }
            led::render_frame(par_buf, host_leds);
        }
    }
}

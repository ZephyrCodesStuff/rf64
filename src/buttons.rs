//! Key matrix reading for MIDI Fighter 64 matching exact C firmware (key.c).
//!
//! Pin Assignments on `ATmega32U4`:
//! - `KEY_CLOCK`: PD7 (PORTD bit 7)
//! - `KEY_LATCH`: PD6 (PORTD bit 6)
use atmega_hal::port::mode::{Floating, Input, Output, PullUp};
use atmega_hal::port::{PC7, PD6, PD7, Pin};

/// Holds the configured shift-register pins for the button matrix.
pub struct ButtonMatrix {
    clock: Pin<Output, PD7>,
    latch: Pin<Output, PD6>,
    data: Pin<Input<PullUp>, PC7>,
}

impl ButtonMatrix {
    /// Initialize button matrix shift register pins using safe HAL abstractions.
    pub fn new(
        clock: Pin<Input<Floating>, PD7>,
        latch: Pin<Input<Floating>, PD6>,
        data: Pin<Input<Floating>, PC7>,
    ) -> Self {
        let mut clock = clock.into_output_high(); // idles high — CD4021B expects CP high before latch
        let mut latch = latch.into_output_high(); // idles high — PL triggers on the high→low edge

        let data = data.into_pull_up_input();

        // CD4021B idle state: Clock HIGH (shifts on rising edge), Latch (PL) LOW (shifting mode).
        clock.set_high();
        latch.set_low();

        Self { clock, latch, data }
    }

    /// Read all 64 buttons immediately.
    ///
    /// Returns a bitmask where bit N = 1 means button N is currently pressed.
    pub fn read_raw(&mut self) -> u64 {
        let mut value: u64 = 0;

        // Pulse Latch (PL) HIGH to asynchronously load parallel button inputs.
        // CD4021B requires minimum 150-250ns pulse width at 5V; 8 cycles @ 16 MHz = 500ns.
        self.latch.set_high();
        avr_device::asm::delay_cycles(8);
        self.latch.set_low();

        for i in 0..64 {
            self.clock.set_low();
            avr_device::asm::delay_cycles(4);

            // Button pressed = Pin is HIGH (1) on the wire
            if self.data.is_high() {
                value |= 1u64 << i;
            }

            avr_device::asm::delay_cycles(4);
            self.clock.set_high(); // shifts next bit out on CD4021B
            avr_device::asm::delay_cycles(4);
        }

        value
    }
}

/// How many timer ticks a button is locked out after an edge transition.
/// At 15,625 Hz (Timer 1 with prescaler 1024 @ 16 MHz), 1 tick = 64 µs.
/// 160 ticks = 160 * 64 µs = 10.24 ms.
pub const DEBOUNCE_TICKS: u16 = 160;

/// Per-button debounce tracker.
///
/// Implements immediate edge response (lowest possible latency) followed
/// by a fixed time-based lockout window measured against a monotonic timer tick.
pub struct Debouncer {
    /// Debounced logical state (bit N = 1 if button N is pressed).
    pub state: u64,
    /// Bitmask indicating which buttons are currently locked out in debounce cooldown.
    lockout_mask: u64,
    /// Tick when each button began its lockout window.
    last_edge_tick: [u16; 64],
}

impl Debouncer {
    pub const fn new() -> Self {
        Self {
            state: 0,
            lockout_mask: 0,
            last_edge_tick: [0; 64],
        }
    }

    /// Update with a new raw sample and current timestamp (from a free-running 15,625 Hz counter).
    ///
    /// Returns `(pressed_edges, released_edges)` bitmasks where bit N = 1 indicates
    /// button N transitioned on this cycle.
    pub fn update(&mut self, raw: u64, now: u16) -> (u64, u64) {
        let mut pressed_edges = 0u64;
        let mut released_edges = 0u64;

        // Fast path: if no buttons are in lockout and raw matches debounced state, no work needed.
        if self.lockout_mask == 0 && raw == self.state {
            return (0, 0);
        }

        let raw_bytes = raw.to_le_bytes();
        let state_bytes = self.state.to_le_bytes();
        let lockout_bytes = self.lockout_mask.to_le_bytes();

        let mut new_state = self.state;
        let mut new_lockout = self.lockout_mask;

        for byte_idx in 0..8 {
            let r_byte = raw_bytes[byte_idx];
            let s_byte = state_bytes[byte_idx];
            let l_byte = lockout_bytes[byte_idx];

            // If no active lockouts in this byte and raw matches state, skip this group of 8 buttons
            if l_byte == 0 && r_byte == s_byte {
                continue;
            }

            for bit_idx in 0..8 {
                let btn = (byte_idx << 3) | bit_idx;
                let mask = 1u64 << btn;

                // Check if button is currently in lockout window
                if (new_lockout & mask) != 0 {
                    if now.wrapping_sub(self.last_edge_tick[btn]) < DEBOUNCE_TICKS {
                        // Still within lockout window: ignore raw changes
                        continue;
                    } else {
                        // Lockout period has elapsed: clear lock
                        new_lockout &= !mask;
                    }
                }

                let is_raw_pressed = (r_byte & (1 << bit_idx)) != 0;
                let was_confirmed = (s_byte & (1 << bit_idx)) != 0;

                if is_raw_pressed != was_confirmed {
                    // Start lockout cooldown
                    self.last_edge_tick[btn] = now;
                    new_lockout |= mask;

                    if is_raw_pressed {
                        new_state |= mask;
                        pressed_edges |= mask;
                    } else {
                        new_state &= !mask;
                        released_edges |= mask;
                    }
                }
            }
        }

        self.state = new_state;
        self.lockout_mask = new_lockout;

        (pressed_edges, released_edges)
    }
}

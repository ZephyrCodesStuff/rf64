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

/// A 64-bit button bitmask represented as an 8-byte array matching
/// the 8 cascaded 8-bit CD4021B shift registers on the MIDI Fighter 64.
///
/// On an 8-bit AVR microcontroller, using `[u8; 8]` avoids register pressure,
/// prevents stack spilling, and avoids expensive 64-bit shifting routines.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
#[repr(transparent)]
pub struct ButtonMask(pub [u8; 8]);

impl ButtonMask {
    pub const EMPTY: Self = Self([0u8; 8]);

    #[inline(always)]
    pub const fn new(bytes: [u8; 8]) -> Self {
        Self(bytes)
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.0 == [0u8; 8]
    }

    #[inline(always)]
    pub fn any(&self) -> bool {
        !self.is_empty()
    }

    #[inline(always)]
    pub const fn is_set(&self, btn: usize) -> bool {
        if btn < 64 {
            (self.0[btn >> 3] & (1 << (btn & 7))) != 0
        } else {
            false
        }
    }

    #[inline(always)]
    pub const fn set(&mut self, btn: usize) {
        if btn < 64 {
            self.0[btn >> 3] |= 1 << (btn & 7);
        }
    }

    #[inline(always)]
    pub const fn clear(&mut self, btn: usize) {
        if btn < 64 {
            self.0[btn >> 3] &= !(1 << (btn & 7));
        }
    }
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
    /// Returns an 8-byte [`ButtonMask`] where byte `N / 8`, bit `N % 8` = 1 means button N is pressed.
    pub fn read_raw(&mut self) -> ButtonMask {
        let mut bytes = [0u8; 8];

        // Pulse Latch (PL) HIGH to asynchronously load parallel button inputs.
        // CD4021B requires minimum 150-250ns pulse width at 5V; 8 cycles @ 16 MHz = 500ns.
        self.latch.set_high();
        avr_device::asm::delay_cycles(8);
        self.latch.set_low();

        for byte in &mut bytes {
            let mut new_byte = 0u8;
            for bit_idx in 0..8 {
                self.clock.set_low();
                avr_device::asm::delay_cycles(4);

                // Button pressed = Pin is HIGH (1) on the wire
                if self.data.is_high() {
                    new_byte |= 1u8 << bit_idx;
                }

                avr_device::asm::delay_cycles(4);
                self.clock.set_high(); // shifts next bit out on CD4021B
                avr_device::asm::delay_cycles(4);
            }
            *byte = new_byte;
        }

        ButtonMask(bytes)
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
    pub state: ButtonMask,
    /// Bitmask indicating which buttons are currently locked out in debounce cooldown.
    lockout_mask: ButtonMask,
    /// Tick when each button began its lockout window.
    last_edge_tick: [u16; 64],
}

impl Debouncer {
    pub const fn new() -> Self {
        Self {
            state: ButtonMask::EMPTY,
            lockout_mask: ButtonMask::EMPTY,
            last_edge_tick: [0; 64],
        }
    }

    /// Update with a new raw sample and current timestamp (from a free-running 15,625 Hz counter).
    ///
    /// Returns `(pressed_edges, released_edges)` bitmasks where bit N = 1 indicates
    /// button N transitioned on this cycle.
    pub fn update(&mut self, raw: ButtonMask, now: u16) -> (ButtonMask, ButtonMask) {
        let mut pressed_edges = ButtonMask::EMPTY;
        let mut released_edges = ButtonMask::EMPTY;

        // Fast path: if no buttons are in lockout and raw matches debounced state, no work needed.
        if self.lockout_mask.is_empty() && raw == self.state {
            return (pressed_edges, released_edges);
        }

        for byte_idx in 0..8 {
            let r_byte = raw.0[byte_idx];
            let s_byte = self.state.0[byte_idx];
            let l_byte = self.lockout_mask.0[byte_idx];

            // If no active lockouts in this byte and raw matches state, skip this group of 8 buttons
            if l_byte == 0 && r_byte == s_byte {
                continue;
            }

            for bit_idx in 0..8 {
                let btn = (byte_idx << 3) | bit_idx;
                let bit_mask = 1u8 << bit_idx;

                // Check if button is currently in lockout window
                if (self.lockout_mask.0[byte_idx] & bit_mask) != 0 {
                    if now.wrapping_sub(self.last_edge_tick[btn]) < DEBOUNCE_TICKS {
                        // Still within lockout window: ignore raw changes
                        continue;
                    } else {
                        // Lockout period has elapsed: clear lock
                        self.lockout_mask.0[byte_idx] &= !bit_mask;
                    }
                }

                let is_raw_pressed = (r_byte & bit_mask) != 0;
                let was_confirmed = (s_byte & bit_mask) != 0;

                if is_raw_pressed != was_confirmed {
                    // Start lockout cooldown
                    self.last_edge_tick[btn] = now;
                    self.lockout_mask.0[byte_idx] |= bit_mask;

                    if is_raw_pressed {
                        self.state.0[byte_idx] |= bit_mask;
                        pressed_edges.0[byte_idx] |= bit_mask;
                    } else {
                        self.state.0[byte_idx] &= !bit_mask;
                        released_edges.0[byte_idx] |= bit_mask;
                    }
                }
            }
        }

        (pressed_edges, released_edges)
    }
}

/// Map spatial grid cell `(row, col)` (0..7, 0..7) to physical button index `0..63`.
///
/// Layout uses Ableton drum rack format:
/// - Left 4 columns (cols 0..3) map to buttons 0..31
/// - Right 4 columns (cols 4..7) map to buttons 32..63
#[inline(always)]
pub const fn cell_to_btn(row: u8, col: u8) -> usize {
    let half_offset = if col >= 4 { 32 } else { 0 };
    let c = (col & 3) as usize;
    half_offset + (row as usize * 4) + c
}

/// Iterate over all set bit indices (0..63) in a 64-bit button bitmask.
///
/// Processes bytes directly in native 8-bit little-endian order using trailing-zero counting
/// (Kernighan bit-twiddling) for optimal 8-bit AVR execution speed.
#[inline(always)]
pub fn for_each_button(mask: ButtonMask, mut f: impl FnMut(u8)) {
    if mask.is_empty() {
        return;
    }
    for (byte_idx, &byte) in mask.0.iter().enumerate() {
        let mut b = byte;
        while b != 0 {
            let bit = b.trailing_zeros() as u8;
            f(((byte_idx as u8) << 3) | bit);
            b &= b - 1;
        }
    }
}

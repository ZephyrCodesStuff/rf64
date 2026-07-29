//! WS2812 RGB LED Driver for MIDI Fighter 64 matching exact hardware timing.
//!
//! Layout & Current Safety Architecture:
//! - 64 Arcade Buttons total (128 physical WS2812 LEDs)
//! - 4 Strands of 32 physical LEDs each:
//!   - Group 0: PB6 (PORTB bit 6, IO address 0x05)
//!   - Group 1: PC6 (PORTC bit 6, IO address 0x08)
//!   - Group 2: PB5 (PORTB bit 5, IO address 0x05)
//!   - Group 3: PB4 (PORTB bit 4, IO address 0x05)

use crate::delay::delay_us;
use core::arch::asm;

pub const NUM_BUTTONS: usize = 64;
pub const LEDS_PER_BUTTON: usize = 2;
pub const BUTTONS_PER_STRAND: usize = 16;
pub const LEDS_PER_STRAND: usize = BUTTONS_PER_STRAND * LEDS_PER_BUTTON; // 32 LEDs per strand
pub const NUM_STRANDS: usize = 4;
pub const TOTAL_LEDS: usize = LEDS_PER_STRAND * NUM_STRANDS; // 128 LEDs total

/// Default safe brightness limit matching C firmware (`MAX_BRIGHTNESS = 48` / ~19% max).
pub const SAFE_MAX_BRIGHTNESS: u8 = 48;

/// Represents an RGB color value (0-255 per channel).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0 };

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b }
    }

    pub fn clamp_brightness(self, max_val: u8) -> Color {
        Color {
            r: if self.r > max_val { max_val } else { self.r },
            g: if self.g > max_val { max_val } else { self.g },
            b: if self.b > max_val { max_val } else { self.b },
        }
    }
}

/// 128-element buffer representing all physical LEDs on the MIDI Fighter 64.
#[derive(Copy, Clone)]
pub struct PhysicalLedBuffer {
    pub leds: [Color; TOTAL_LEDS],
}

impl PhysicalLedBuffer {
    pub const fn new() -> Self {
        PhysicalLedBuffer {
            leds: [Color::BLACK; TOTAL_LEDS],
        }
    }

    pub fn clear(&mut self) {
        self.leds.fill(Color::BLACK);
    }

    /// Set the two physical LEDs of a button to different colors.
    pub fn set_button_split(&mut self, button_idx: usize, led0: Color, led1: Color) {
        if button_idx < NUM_BUTTONS {
            let base_led = button_idx * LEDS_PER_BUTTON;
            self.leds[base_led] = led0.clamp_brightness(SAFE_MAX_BRIGHTNESS);
            self.leds[base_led + 1] = led1.clamp_brightness(SAFE_MAX_BRIGHTNESS);
        }
    }
}

// IO Port addresses on ATmega32U4: PORTB = 0x05, PORTC = 0x08
const PORTB_IO: u8 = 0x05;
const PORTC_IO: u8 = 0x08;

/// Generic WS2812 bit-bang byte transmission for specified I/O PORT and PIN.
/// Direct sbi/cbi instructions guarantee exact cycle-accurate WS2812 timing.
///
/// These cannot be ported to `delay` functions because we need sub-microsecond precision.
#[inline(always)]
unsafe fn send_byte_pin<const PORT: u8, const PIN: u8>(byte: u8) {
    let mut mask: u8 = 0x80;
    while mask != 0 {
        if (byte & mask) != 0 {
            // Bit 1: sbi high, wait 6 NOPs (~0.45us), cbi low, wait 10 NOPs (~0.65us)
            unsafe {
                asm!(
                    "sbi {port}, {pin}",
                    "nop", "nop", "nop", "nop", "nop", "nop",
                    "cbi {port}, {pin}",
                    "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop",
                    port = const PORT,
                    pin = const PIN,
                    options(nomem, nostack)
                );
            }
        } else {
            // Bit 0: sbi high, cbi low immediately (~0.15us), wait 10 NOPs (~0.65us)
            unsafe {
                asm!(
                    "sbi {port}, {pin}",
                    "cbi {port}, {pin}",
                    "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop",
                    port = const PORT,
                    pin = const PIN,
                    options(nomem, nostack)
                );
            }
        }
        mask >>= 1;
    }
}

#[inline(always)]
unsafe fn send_byte_pb6(byte: u8) {
    unsafe { send_byte_pin::<PORTB_IO, 6>(byte) };
}

#[inline(always)]
unsafe fn send_byte_pc6(byte: u8) {
    unsafe { send_byte_pin::<PORTC_IO, 6>(byte) };
}

#[inline(always)]
unsafe fn send_byte_pb5(byte: u8) {
    unsafe { send_byte_pin::<PORTB_IO, 5>(byte) };
}

#[inline(always)]
unsafe fn send_byte_pb4(byte: u8) {
    unsafe { send_byte_pin::<PORTB_IO, 4>(byte) };
}

/// Drives WS2812 LED strands on the MIDI Fighter 64 hardware.
pub struct LedDriver {
    _private: (),
}

impl LedDriver {
    pub fn new() -> Self {
        LedDriver { _private: () }
    }

    /// Transmit strand 0 (LEDs 0..31) on PB6. ~0.96ms. Call poll() after each strand.
    pub fn send_strand0(&self, buffer: &PhysicalLedBuffer) {
        unsafe {
            for idx in 0..LEDS_PER_STRAND {
                let color = buffer.leds[idx];
                send_byte_pb6(color.g);
                send_byte_pb6(color.r);
                send_byte_pb6(color.b);
            }
        }
    }

    /// Transmit strand 1 (LEDs 32..63) on PC6. ~0.96ms. Call poll() after each strand.
    pub fn send_strand1(&self, buffer: &PhysicalLedBuffer) {
        unsafe {
            for idx in 0..LEDS_PER_STRAND {
                let color = buffer.leds[LEDS_PER_STRAND + idx];
                send_byte_pc6(color.g);
                send_byte_pc6(color.r);
                send_byte_pc6(color.b);
            }
        }
    }

    /// Transmit strand 2 (LEDs 64..95) on PB5. ~0.96ms. Call poll() after each strand.
    pub fn send_strand2(&self, buffer: &PhysicalLedBuffer) {
        unsafe {
            for idx in 0..LEDS_PER_STRAND {
                let color = buffer.leds[LEDS_PER_STRAND * 2 + idx];
                send_byte_pb5(color.g);
                send_byte_pb5(color.r);
                send_byte_pb5(color.b);
            }
        }
    }

    /// Transmit strand 3 (LEDs 96..127) on PB4. ~0.96ms. Call poll() after each strand.
    pub fn send_strand3(&self, buffer: &PhysicalLedBuffer) {
        unsafe {
            for idx in 0..LEDS_PER_STRAND {
                let color = buffer.leds[LEDS_PER_STRAND * 3 + idx];
                send_byte_pb4(color.g);
                send_byte_pb4(color.r);
                send_byte_pb4(color.b);
            }
        }
    }

    /// Latch the frame by holding all lines LOW for >50µs.
    /// Call this once after all 4 strands are sent.
    pub fn latch_frame(&self) {
        delay_us(80);
    }
}

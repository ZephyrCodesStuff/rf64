//! WS2812 RGB LED Driver for MIDI Fighter 64 matching exact hardware timing.
//!
//! Layout & parallelism architecture:
//! - 64 Arcade Buttons total (128 physical WS2812 LEDs)
//! - 4 Strands of 32 physical LEDs each:
//!   - Strand 0: PB6 (PORTB bit 6, IO 0x05)  ┐
//!   - Strand 2: PB5 (PORTB bit 5, IO 0x05)  ├─ sent in PARALLEL via `out PORTB`
//!   - Strand 3: PB4 (PORTB bit 4, IO 0x05)  ┘
//!   - Strand 1: PC6 (PORTC bit 6, IO 0x08)  ──── sent sequentially

use crate::delay::delay_us;
use core::arch::asm;

#[allow(dead_code)]
pub const NUM_BUTTONS: usize = 64;
pub const LEDS_PER_BUTTON: usize = 2;
pub const BUTTONS_PER_STRAND: usize = 16;
pub const LEDS_PER_STRAND: usize = BUTTONS_PER_STRAND * LEDS_PER_BUTTON; // 32 LEDs per strand
pub const NUM_STRANDS: usize = 4;
pub const TOTAL_LEDS: usize = LEDS_PER_STRAND * NUM_STRANDS; // 128 LEDs total

/// Max color units (R+G+B) across all 128 LEDs to keep power draw under ~450mA total.
pub const SAFE_MAX_COLOR_SUM: u32 = 6200;

/// Max brightness for a single color channel (approx 30% of 255) to avoid blinding.
pub const SAFE_MAX_PIXEL_COMPONENT: u8 = 76;

/// Represents an RGB color value (0–255 per channel).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    /// Used to clear the LED buffer to black (all off) before each frame.
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0 };

    /// Used for the bootloader indicator checkerboard pattern.
    ///
    /// Orange is chosen to tell the user they got here from OUR firmware,
    /// and not official one (since that one uses blue).
    pub const ORANGE: Self = Self {
        r: 255,
        g: 80,
        b: 0,
    };

    /// Used for panic handler red checkerboard indicator.
    pub const RED: Self = Self { r: 255, g: 0, b: 0 };

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Dynamically scale brightness by a fraction (scale / 256).
    /// If an original color channel was > 0, we ensure it never rounds down to 0,
    /// so that very dim colors aren't entirely extinguished by scaling.
    pub const fn scale_brightness(self, scale: u16) -> Self {
        let sr = (self.r as u16 * scale) >> 8;
        let sg = (self.g as u16 * scale) >> 8;
        let sb = (self.b as u16 * scale) >> 8;

        Self {
            r: if self.r > 0 && sr == 0 { 1 } else { sr as u8 },
            g: if self.g > 0 && sg == 0 { 1 } else { sg as u8 },
            b: if self.b > 0 && sb == 0 { 1 } else { sb as u8 },
        }
    }
}

/// Calculate dynamic power and brightness scale factor (0-256) across all LEDs.
pub fn compute_safe_scale(host_leds: &[Color; TOTAL_LEDS]) -> u16 {
    let mut total_sum: u32 = 0;
    let mut max_component: u8 = 0;
    for c in host_leds.iter() {
        total_sum += c.r as u32 + c.g as u32 + c.b as u32;
        if c.r > max_component {
            max_component = c.r;
        }
        if c.g > max_component {
            max_component = c.g;
        }
        if c.b > max_component {
            max_component = c.b;
        }
    }

    let power_scale = if total_sum > SAFE_MAX_COLOR_SUM {
        ((SAFE_MAX_COLOR_SUM * 256) / total_sum) as u16
    } else {
        256
    };

    let bright_scale = if max_component > SAFE_MAX_PIXEL_COMPONENT {
        (SAFE_MAX_PIXEL_COMPONENT as u16 * 256) / max_component as u16
    } else {
        256
    };

    if power_scale < bright_scale {
        power_scale
    } else {
        bright_scale
    }
}

// ── Parallel bit buffer ───────────────────────────────────────────────────────

/// Pre-computed PORTB output masks for parallel WS2812 transmission.
///
/// Contains 768 bytes representing 32 LED positions × 3 WS2812 bytes (G, R, B)
/// × 8 bits, MSB first. Each byte encodes the PORTB mid-phase value for one
/// simultaneous bit across strands 0 (PB6), 2 (PB5), and 3 (PB4):
///
/// ```text
/// masks[n] = (strand0_bit_n << 6) | (strand2_bit_n << 5) | (strand3_bit_n << 4)
/// ```
///
/// This cannot be a compile-time constant because LED colours are driven by
/// live MIDI messages from the host and change every frame. The struct IS
/// `const`-constructible (all zeros), so it can live in BSS as a `static mut`.
///
/// Use [`fill_parallel_buffer_into`] to populate it before each transmission.
pub struct ParallelBitBuffer {
    pub masks: [u8; 768],
}

impl ParallelBitBuffer {
    /// Construct a zeroed buffer. Suitable for `static mut` initialisation.
    pub const fn new() -> Self {
        Self { masks: [0u8; 768] }
    }
}

/// Pre-compute the 768 PORTB mid-phase masks from the full LED colour array.
///
/// Runs in unconstrained Rust — no timing requirements. Call once per frame,
/// before [`LedDriver::send_portb_parallel`].
///
/// Strand → `host_leds` slice mapping:
/// - Strand 0 (PB6, bit 6): `host_leds[  0 ..  32]`
/// - Strand 1 (PC6):        `host_leds[ 32 ..  64]`  ← handled separately
/// - Strand 2 (PB5, bit 5): `host_leds[ 64 ..  96]`
/// - Strand 3 (PB4, bit 4): `host_leds[ 96 .. 128]`
pub fn fill_parallel_buffer_into(
    buf: &mut ParallelBitBuffer,
    host_leds: &[Color; TOTAL_LEDS],
    scale: u16,
) {
    let mut idx = 0usize;

    for led_pos in 0..LEDS_PER_STRAND {
        // Scale brightness dynamically.
        let c0 = host_leds[led_pos].scale_brightness(scale);
        let c2 = host_leds[LEDS_PER_STRAND * 2 + led_pos].scale_brightness(scale);
        let c3 = host_leds[LEDS_PER_STRAND * 3 + led_pos].scale_brightness(scale);

        // WS2812 wire order: G → R → B.
        for (b0, b2, b3) in [(c0.g, c2.g, c3.g), (c0.r, c2.r, c3.r), (c0.b, c2.b, c3.b)] {
            // MSB first (bit 7 down to 0).
            for bit in (0..8u8).rev() {
                buf.masks[idx] =
                    ((b0 >> bit) & 1) << 6 | ((b2 >> bit) & 1) << 5 | ((b3 >> bit) & 1) << 4;
                idx += 1;
            }
        }
    }
}

// ── I/O port addresses on ATmega32U4 ─────────────────────────────────────────
const PORTC_IO: u8 = 0x08;

// ── WS2812 bit-bang: unrolled SBRS technique (PC6 / strand 1 only) ───────────

/// Generic WS2812 bit-bang byte transmission for a single I/O PORT and PIN.
/// Direct sbi/cbi instructions guarantee exact cycle-accurate WS2812 timing.
///
/// These cannot be ported to `delay` functions because we need sub-microsecond precision.
///
/// # Implementation — unrolled, branch-free SBRS technique
///
/// The original implementation used a `while mask != 0` loop with an `if/else`
/// branch to select between the two asm paths. That added ~5 overhead cycles per
/// bit (mask shift + loop-back branch + loop-test), totalling ~3 072 wasted cycles
/// across a full 128-LED frame (~192 µs at 16 MHz).
///
/// This version uses the AVR `sbrs` (Skip if Bit in Register Set) instruction
/// to implement a branch-free select, and unrolls all 8 bits with a macro so
/// there is no loop counter, no shift, and no jump.
///
/// ## Timing per bit at 16 MHz (1 cycle = 62.5 ns):
///
/// ```text
/// sbi  PORT, PIN       ; 2 cy → pin goes HIGH
/// sbrs byte, N         ; 2 cy (skip, bit=1) | 1 cy (fall-through, bit=0)
/// cbi  PORT, PIN       ; [bit=0: 2 cy → pin goes LOW early] | [bit=1: SKIPPED]
/// nop × 4              ; 4 cy padding
/// cbi  PORT, PIN       ; [bit=1: 2 cy → pin goes LOW] | [bit=0: 2 cy, harmless re-assert]
/// nop × 10             ; 10 cy LOW hold
/// ```
///
/// | Signal | Cycles | Time    | Previous |
/// |--------|--------|---------|----------|
/// | T1H    | 8 cy   | 500 ns  | 500 ns   | (unchanged)
/// | T0H    | 3 cy   | 187 ns  | 125 ns   | (slightly improved)
#[inline(always)]
unsafe fn send_byte_pin<const PORT: u8, const PIN: u8>(byte: u8) {
    // Emit one bit using the SBRS skip trick.
    //
    // `sbrs b, N` skips the immediately following instruction when bit N of
    // register `b` is set. For bit=1 it skips the early `cbi`, letting the
    // pin stay HIGH through 4 NOPs before the unconditional `cbi` pulls it
    // LOW (T1H = 8 cycles). For bit=0 it falls through to the early `cbi`
    // immediately (T0H = 3 cycles).
    macro_rules! send_bit {
        ($bit:literal) => {
            unsafe {
                asm!(
                    "sbi {port}, {pin}",          // pin HIGH
                    "sbrs {b}, {bit}",             // skip early cbi if bit=1
                    "cbi {port}, {pin}",           // [bit=0] pin LOW  (T0H ≈ 3 cy)
                    "nop", "nop", "nop", "nop",    // timing pad
                    "cbi {port}, {pin}",           // [bit=1] pin LOW  (T1H ≈ 8 cy)
                    "nop", "nop", "nop", "nop", "nop",
                    "nop", "nop", "nop", "nop", "nop", // LOW hold (10 cy)
                    port = const PORT,
                    pin  = const PIN,
                    b    = in(reg) byte,
                    bit  = const $bit,
                    options(nomem, nostack),
                );
            }
        };
    }

    // MSB-first, fully unrolled — no loop, no mask register, no branch.
    send_bit!(7);
    send_bit!(6);
    send_bit!(5);
    send_bit!(4);
    send_bit!(3);
    send_bit!(2);
    send_bit!(1);
    send_bit!(0);
}

#[inline(always)]
pub unsafe fn send_byte_pc6(byte: u8) {
    unsafe { send_byte_pin::<PORTC_IO, 6>(byte) };
}

// ── LED Driver ────────────────────────────────────────────────────────────────

/// Drives WS2812 LED strands on the MIDI Fighter 64 hardware.
pub struct LedDriver {
    _private: (),
}

impl LedDriver {
    pub const fn new() -> Self {
        Self { _private: () }
    }

    /// Transmit strands 0 (PB6), 2 (PB5), and 3 (PB4) simultaneously in ~0.91 ms.
    ///
    /// Reads from a pre-computed [`ParallelBitBuffer`] produced by
    /// [`fill_parallel_buffer_into`]. Three PORTB data lines are driven by a single
    /// `out PORTB, mask` per phase — one pass replaces three sequential strand calls.
    ///
    /// ## Per-bit timing at 16 MHz (1 cycle = 62.5 ns)
    ///
    /// ```text
    ///  out PORTB, 0x70   ; 1 cy:  PB6 | PB5 | PB4 = HIGH
    ///  nop               ; 1 cy:  │
    ///  ld mid, Z+        ; 2 cy:  ├── T0H window: 4 cy = 250 ns ──┐
    ///  out PORTB, mid    ; 1 cy:  0-bit pins go LOW               │
    ///  nop × 4           ; 4 cy:  │                               │
    ///  out PORTB, 0x00   ; 1 cy:  all pins go LOW    T1H: 9 cy = 562 ns
    ///  nop × 5           ; 5 cy:  │
    ///  sbiw cnt, 1       ; 2 cy:  ├── LOW hold ───────────────────┘
    ///  brne loop         ; 2 cy:  │  (T0L ≈ 14 cy = 875 ns, T1L ≈ 9 cy = 562 ns)
    /// ```
    ///
    /// The `ld Z+` load is placed inside the T0H window so it serves as both
    /// a data fetch and a timing delay — zero wasted cycles.
    pub fn send_portb_parallel(&self, buf: &ParallelBitBuffer) {
        // Safety: buf.masks is a valid, initialised 768-byte array.
        // Z (r31:r30) is used as the auto-incrementing data pointer.
        // sbiw operates on the reg_iw pair chosen by the compiler for `cnt`.
        // options(nostack): we touch SRAM via `ld Z+` but not the stack.
        let z_addr = buf.masks.as_ptr() as usize;
        unsafe {
            asm!(
                "99:",
                "out 0x05, {high}",          // 1 cy: ALL HIGH  (PB6 | PB5 | PB4)
                "nop",                        // 1 cy: T0H pad
                "ld {mid}, Z+",              // 2 cy: load mid-mask, advance Z  ← T0H pad
                "out 0x05, {mid}",           // 1 cy: 0-bit pins LOW  → T0H = 4 cy = 250 ns
                "nop",                        // 1 cy: T1H pad
                "nop",                        // 1 cy
                "nop",                        // 1 cy
                "nop",                        // 1 cy
                "out 0x05, {zero}",          // 1 cy: ALL LOW         → T1H = 9 cy = 562 ns
                "nop",                        // 1 cy: LOW hold
                "nop",                        // 1 cy
                "nop",                        // 1 cy
                "nop",                        // 1 cy
                "nop",                        // 1 cy
                "sbiw {cnt}, 1",             // 2 cy: decrement 16-bit loop counter
                "brne 99b",                  // 2 cy (taken) / 1 cy (not taken)
                high = in(reg) 0x70u8,       // PB6 | PB5 | PB4 HIGH mask (constant)
                zero = in(reg) 0x00u8,       // ALL-LOW mask (constant)
                mid  = out(reg) _,           // scratch register for loaded mask
                cnt  = inout(reg_iw) 768u16 => _, // 16-bit loop counter (sbiw-compatible pair)
                inout("Z") z_addr => _,      // Z = data pointer; modified by ld Z+
                options(nostack),
            );
        }
    }

    /// Transmit strand 1 (LEDs 32..63) on PC6. ~0.96 ms. Call `poll()` after.
    ///
    /// Accepts a slice of exactly [`LEDS_PER_STRAND`] (32) colours, brightness-clamped
    /// internally. Pass `&host_leds[LEDS_PER_STRAND..LEDS_PER_STRAND * 2]`.
    pub fn send_strand1(&self, leds: &[Color], scale: u16) {
        unsafe {
            for color in leds {
                let color = color.scale_brightness(scale);
                send_byte_pc6(color.g);
                send_byte_pc6(color.r);
                send_byte_pc6(color.b);
            }
        }
    }

    /// Latch the frame by holding all lines LOW for >50 µs.
    /// Call once after all strands are sent.
    pub fn latch_frame(&self) {
        delay_us(80);
    }

    /// Complete frame render pipeline: computes safe scaling, fills parallel bit buffer,
    /// transmits parallel PORTB strands, polls USB, transmits sequential PC6 strand,
    /// polls USB, and latches the frame.
    pub fn render_frame(
        &self,
        par_buf: &mut ParallelBitBuffer,
        host_leds: &[Color; TOTAL_LEDS],
    ) {
        let final_scale = compute_safe_scale(host_leds);

        fill_parallel_buffer_into(par_buf, host_leds, final_scale);

        self.send_portb_parallel(par_buf);
        crate::usb::poll();

        self.send_strand1(
            &host_leds[LEDS_PER_STRAND..LEDS_PER_STRAND * 2],
            final_scale,
        );
        crate::usb::poll();

        self.latch_frame();
    }

    /// Send a fixed checkerboard pattern of `color` (on even buttons) and BLACK (on odd buttons).
    ///
    /// This is used for internal states of the firmware:
    /// - Bootloader entry: ORANGE checkerboard
    /// - Panic handler: RED checkerboard
    pub fn send_checkerboard_direct(&self, par_buf: &mut ParallelBitBuffer, color: Color) {
        // Pre-compute scaled colour bytes (GRB wire order for WS2812).
        // For a 32-LED checkerboard at 16 ON buttons × 2 LEDs × R=255:
        // total = 16 * 2 * 255 = 8160 units. Apply the same power cap as normal.
        //
        // Simple fixed scale: 16 ON buttons × 2 LEDs × (r+g+b) vs SAFE_MAX_COLOR_SUM.
        let num_on_leds: u32 = 32; // 16 buttons ON × 2 LEDs each per strand pair
        let per_led_sum = color.r as u32 + color.g as u32 + color.b as u32;
        let total_sum = per_led_sum * num_on_leds;
        let scale: u16 = if total_sum > SAFE_MAX_COLOR_SUM {
            ((SAFE_MAX_COLOR_SUM * 256) / total_sum) as u16
        } else {
            256
        };
        let c = color.scale_brightness(scale);

        // Fill par_buf for strands 0, 2, 3 simultaneously (PORTB parallel).
        // Each mask byte: bit6=strand0, bit5=strand2, bit4=strand3, for the same LED position.
        let mut idx = 0usize;
        for led_pos in 0..LEDS_PER_STRAND {
            let btn_in_strand = led_pos / 2; // 0..15

            // Strand 0: global button = 0..15 (rows 0-1, row = btn/8, col = btn%8)
            let btn0 = btn_in_strand; // buttons 0..15
            let on0 = (btn0 / 8 + btn0 % 8) % 2 == 0;
            let (g0, r0, b0) = if on0 {
                (c.g, c.r, c.b)
            } else {
                (0u8, 0u8, 0u8)
            };

            // Strand 2: global button = 32..47 (rows 4-5)
            let btn2 = 32 + btn_in_strand;
            let on2 = (btn2 / 8 + btn2 % 8) % 2 == 0;
            let (g2, r2, b2) = if on2 {
                (c.g, c.r, c.b)
            } else {
                (0u8, 0u8, 0u8)
            };

            // Strand 3: global button = 48..63 (rows 6-7)
            let btn3 = 48 + btn_in_strand;
            let on3 = (btn3 / 8 + btn3 % 8) % 2 == 0;
            let (g3, r3, b3) = if on3 {
                (c.g, c.r, c.b)
            } else {
                (0u8, 0u8, 0u8)
            };

            // Fill the 24 mask bytes for this LED position (8 bits × 3 WS2812 bytes) into par_buf.
            for (b0v, b2v, b3v) in [(g0, g2, g3), (r0, r2, r3), (b0, b2, b3)] {
                for bit in (0..8u8).rev() {
                    par_buf.masks[idx] =
                        ((b0v >> bit) & 1) << 6 | ((b2v >> bit) & 1) << 5 | ((b3v >> bit) & 1) << 4;
                    idx += 1;
                }
            }
        }
        self.send_portb_parallel(par_buf);

        // Send strand 1 (PC6) sequentially
        unsafe {
            for led_pos in 0..LEDS_PER_STRAND {
                let btn_in_strand = led_pos / 2;
                let btn1 = 16 + btn_in_strand;
                let on1 = (btn1 / 8 + btn1 % 8) % 2 == 0;
                let (g, r, b) = if on1 { (c.g, c.r, c.b) } else { (0, 0, 0) };
                send_byte_pc6(g);
                send_byte_pc6(r);
                send_byte_pc6(b);
            }
        }

        // Display the frame
        self.latch_frame();
    }
}

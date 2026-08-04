//! FastRGB handling for high-speed SysEx LED updates.
//!
//! Provides the decompression algorithms used by Apollo Studio and other
//! high-performance host software to update the entire grid with minimal USB overhead.

use crate::led::{Color, TOTAL_LEDS};

const BUTTON_ID_FLAGS: u8 = 0x3F;

/// Clear all LEDs.
pub fn fastrgb_clear(host_leds: &mut [Color; TOTAL_LEDS]) {
    host_leds.fill(Color::BLACK);
}

/// Set a virtual button (0-63) to a 6-bit RGB value (0-63).
///
/// We shift the 6-bit color left by 2 to map it to the 8-bit (0-255) range
/// expected by the LED driver, which will automatically handle brightness scaling.
#[inline(always)]
const fn fastrgb_set_unsafe(p: u8, r: u8, g: u8, b: u8, host_leds: &mut [Color; TOTAL_LEDS]) {
    // Only map valid pad indices (0-63)
    if p < 64 {
        let r8 = if r == 0 { 0 } else { (r << 2) | (r >> 4) };
        let g8 = if g == 0 { 0 } else { (g << 2) | (g >> 4) };
        let b8 = if b == 0 { 0 } else { (b << 2) | (b >> 4) };

        let c = Color::new(r8, g8, b8);

        // Two LEDs per button. Mapping from midi.rs channel handling.
        let base_led = (p as usize) * 2;
        if base_led + 1 < TOTAL_LEDS {
            host_leds[base_led] = c;
            host_leds[base_led + 1] = c;
        }
    }
}

/// Process uncompressed list of FastRGB updates (F0 6F).
/// Each chunk is 4 bytes: `[pad_id, r, g, b]`.
pub fn fastrgb_list(data: &[u8], host_leds: &mut [Color; TOTAL_LEDS]) {
    for chunk in data.as_chunks::<4>().0 {
        let p = chunk[0];
        let r = chunk[1] & 0x3F;
        let g = chunk[2] & 0x3F;
        let b = chunk[3] & 0x3F;
        fastrgb_set_unsafe(p & BUTTON_ID_FLAGS, r, g, b, host_leds);
    }
}

/// Decompress FastRGB payload (F0 5F) and apply it to the LED grid.
///
/// This uses the Apollo Studio compression format where `n` (number of targets)
/// is packed into the MSBs of `r`, `g`, `b`.
pub fn fastrgb_decompress(data: &[u8], host_leds: &mut [Color; TOTAL_LEDS]) {
    let mut i = 0;
    let len = data.len();

    while i < len {
        if i + 3 > len {
            break;
        }

        let mut r = data[i];
        let mut g = data[i + 1];
        let mut b = data[i + 2];
        i += 3;

        // Reconstruct 'n' from the 6th bits (0x40) of r, g, b.
        let mut n = ((r & 0x40) >> 4) | ((g & 0x40) >> 5) | ((b & 0x40) >> 6);
        if n == 0 {
            if i < len {
                n = data[i];
                i += 1;
            } else {
                break;
            }
        }

        r &= 0x3F;
        g &= 0x3F;
        b &= 0x3F;

        for _ in 0..n {
            if i >= len {
                break;
            }
            let x = data[i];
            i += 1;

            if (x & 0b01110000) != 0b01100000 {
                // Standard button
                fastrgb_set_unsafe(x & BUTTON_ID_FLAGS, r, g, b, host_leds);

                // Symmetric copy
                if (x & 0b01000000) != 0 {
                    let x_inv = !x & BUTTON_ID_FLAGS;
                    fastrgb_set_unsafe(x_inv, r, g, b, host_leds);

                    // Quadrant mirror
                    if (x & 0b00100000) != 0 {
                        fastrgb_set_unsafe(
                            (x & 0b00011100) | (x_inv & 0b00000011),
                            r,
                            g,
                            b,
                            host_leds,
                        );
                        fastrgb_set_unsafe(
                            (x & 0b00100011) | (x_inv & 0b00011100),
                            r,
                            g,
                            b,
                            host_leds,
                        );
                    }
                }
            } else if (x & 0b00001000) != 0 {
                // Entire column
                let col = x & if (x & 0b00000100) != 0 {
                    0b00100011
                } else {
                    0b00000011
                };
                for k in 0..core::hint::black_box(8) {
                    fastrgb_set_unsafe(col | (k << 2), r, g, b, host_leds);
                }
            } else {
                // Entire row
                let row = (x & 0b00000111) << 2;
                for k in 0..core::hint::black_box(4) {
                    fastrgb_set_unsafe(row | k, r, g, b, host_leds);
                    fastrgb_set_unsafe(row | 0b00100000 | k, r, g, b, host_leds);
                }
            }
        }
    }
}

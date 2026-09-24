//! GPIO abstractions for the 4 LED strand pins of the MIDI Fighter 64.
//!
//! Pin Assignments (from hardware schematic / C source):
//! - Group 0: PB6 (Bit 6 of PORTB)
//! - Group 1: PC6 (Bit 6 of PORTC)
//! - Group 2: PB5 (Bit 5 of PORTB)
//! - Group 3: PB4 (Bit 4 of PORTB)

use atmega_hal::pac::{PORTB, PORTC};

#[non_exhaustive] // Prevent manual instantiation
pub struct LedPins {}

impl LedPins {
    /// Initialize PB4, PB5, PB6, and PC6 as digital outputs.
    pub fn setup(port_b: &PORTB, port_c: &PORTC) -> Self {
        // Configure the LED pins as output pins
        port_b.ddrb().modify(|_, w| {
            w.pb4().set_bit();
            w.pb5().set_bit();
            w.pb6().set_bit();

            w
        });

        port_c.ddrc().modify(|_, w| {
            w.pc6().set_bit();

            w
        });

        // Ensure all pins start fully low
        port_b.portb().modify(|_, w| {
            w.pb4().clear_bit();
            w.pb5().clear_bit();
            w.pb6().clear_bit();

            w
        });

        port_c.portc().modify(|_, w| {
            w.pc6().clear_bit();

            w
        });

        Self {}
    }
}

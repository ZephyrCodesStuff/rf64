//! GPIO abstractions for the 4 LED strand pins of the MIDI Fighter 64.
//!
//! Pin Assignments (from hardware schematic / C source):
//! - Group 0: PB6 (Bit 6 of PORTB)
//! - Group 1: PC6 (Bit 6 of PORTC)
//! - Group 2: PB5 (Bit 5 of PORTB)
//! - Group 3: PB4 (Bit 4 of PORTB)

use atmega_hal::pac::{PORTB, PORTC};

pub struct LedPins {
    _private: (),
}

impl LedPins {
    /// Initialize PB4, PB5, PB6, and PC6 as digital outputs.
    pub fn init(portb: &PORTB, portc: &PORTC) -> Self {
        // Set PB4, PB5, PB6 as outputs (DDRB bits 4, 5, 6 = 1)
        portb
            .ddrb()
            .modify(|r, w| unsafe { w.bits(r.bits() | (1 << 4) | (1 << 5) | (1 << 6)) });

        // Set PC6 as output (DDRC bit 6 = 1)
        portc
            .ddrc()
            .modify(|r, w| unsafe { w.bits(r.bits() | (1 << 6)) });

        // Ensure all pins start LOW
        portb
            .portb()
            .modify(|r, w| unsafe { w.bits(r.bits() & !((1 << 4) | (1 << 5) | (1 << 6))) });
        portc
            .portc()
            .modify(|r, w| unsafe { w.bits(r.bits() & !(1 << 6)) });

        LedPins { _private: () }
    }
}

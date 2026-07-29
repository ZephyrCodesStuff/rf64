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

#[allow(dead_code)]
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

    /// Set Group 0 (PB6) state
    #[inline(always)]
    pub fn set_group0_high(&self, portb: &PORTB) {
        portb
            .portb()
            .modify(|r, w| unsafe { w.bits(r.bits() | (1 << 6)) });
    }

    #[inline(always)]
    pub fn set_group0_low(&self, portb: &PORTB) {
        portb
            .portb()
            .modify(|r, w| unsafe { w.bits(r.bits() & !(1 << 6)) });
    }

    /// Set Group 1 (PC6) state
    #[inline(always)]
    pub fn set_group1_high(&self, portc: &PORTC) {
        portc
            .portc()
            .modify(|r, w| unsafe { w.bits(r.bits() | (1 << 6)) });
    }

    #[inline(always)]
    pub fn set_group1_low(&self, portc: &PORTC) {
        portc
            .portc()
            .modify(|r, w| unsafe { w.bits(r.bits() & !(1 << 6)) });
    }

    /// Set Group 2 (PB5) state
    #[inline(always)]
    pub fn set_group2_high(&self, portb: &PORTB) {
        portb
            .portb()
            .modify(|r, w| unsafe { w.bits(r.bits() | (1 << 5)) });
    }

    #[inline(always)]
    pub fn set_group2_low(&self, portb: &PORTB) {
        portb
            .portb()
            .modify(|r, w| unsafe { w.bits(r.bits() & !(1 << 5)) });
    }

    /// Set Group 3 (PB4) state
    #[inline(always)]
    pub fn set_group3_high(&self, portb: &PORTB) {
        portb
            .portb()
            .modify(|r, w| unsafe { w.bits(r.bits() | (1 << 4)) });
    }

    #[inline(always)]
    pub fn set_group3_low(&self, portb: &PORTB) {
        portb
            .portb()
            .modify(|r, w| unsafe { w.bits(r.bits() & !(1 << 4)) });
    }
}

//! Delay utilities for 16 MHz ATmega32U4 using atmega_hal.

use atmega_hal::prelude::*;

pub type Delay = atmega_hal::delay::Delay<atmega_hal::clock::MHz16>;

#[inline(always)]
pub fn delay_us(us: u16) {
    Delay::new().delay_us(us);
}

#[inline(always)]
pub fn delay_ms(ms: u16) {
    Delay::new().delay_ms(ms);
}

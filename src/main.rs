#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]

mod bootloader;
mod delay;
mod gpio;
mod keys;
mod led;
mod midi;
mod usb;

use atmega_hal::Peripherals;
use gpio::LedPins;
use keys::{key_read_raw, key_setup};
use led::{Color, LedDriver, NUM_BUTTONS, PhysicalLedBuffer, TOTAL_LEDS};
use midi::{ButtonState, process_incoming_midi, process_keys};

/// Set CPU prescaler to 1 (16 MHz full speed).
#[inline(always)]
fn cpu_init_16mhz() {
    unsafe {
        core::arch::asm!(
            "sts 0x61, {enable}",
            "sts 0x61, {div1}",
            enable = in(reg) 0x80u8, // CLKPCE (bit 7)
            div1   = in(reg) 0x00u8, // division factor 1 (16 MHz)
            options(nomem, nostack)
        );
    }
}

/// Disable JTAG on MCUCR to free PORTC/PORTF pins for GPIO.
#[inline(always)]
fn disable_jtag() {
    unsafe {
        core::arch::asm!(
            "sts 0x55, {jtd}",
            "sts 0x55, {jtd}",
            jtd = in(reg) 0x80u8, // JTD (bit 7)
            options(nomem, nostack)
        );
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[atmega_hal::entry]
fn main() -> ! {
    // -------------------------------------------------------------------------
    // 1. Hardware safeguards: WDT disable, bootloader check, 16 MHz CPU, JTAG disable
    // -------------------------------------------------------------------------
    bootloader::bootloader_jump_check();
    cpu_init_16mhz();
    disable_jtag();

    // -------------------------------------------------------------------------
    // 2. Initialize HAL peripherals, key matrix pins, & LED driver
    // -------------------------------------------------------------------------
    let dp = Peripherals::take().unwrap();
    let _led_pins = LedPins::init(&dp.PORTB, &dp.PORTC);

    // Disable interrupts during initialization to avoid race conditions
    avr_device::interrupt::disable();

    // Initialize 48MHz USB PLL clock and USB bus allocator
    usb::init_usb_pll();
    let bus_allocator = usb::create_usb_bus(dp.USB_DEVICE);
    let mut usb_stack = usb::UsbMidiStack::new(&bus_allocator);

    let pins = atmega_hal::pins!(dp);

    // Initialize key matrix pins using HAL pin abstractions
    key_setup(pins.pd7, pins.pd6, pins.pc7);

    // Check if Button 0 is held down at startup to jump into DFU bootloader
    let initial_keys = key_read_raw();
    if bootloader::bootloader_combo_held(initial_keys) {
        bootloader::jump_to_bootloader();
    }

    let led_driver = LedDriver::new();
    let mut buffer = PhysicalLedBuffer::new();
    let mut btn_state = ButtonState::new();

    // Colors per strand (all capped at <= 20% max brightness)
    let strand_colors = [
        Color::RED,   // Strand 0 (Buttons 0..15)
        Color::GREEN, // Strand 1 (Buttons 16..31)
        Color::CYAN,  // Strand 2 (Buttons 32..47)
        Color::WHITE, // Strand 3 (Buttons 48..63)
    ];

    let mut host_leds: [Color; TOTAL_LEDS] = [Color::BLACK; TOTAL_LEDS];

    // -------------------------------------------------------------------------
    // 3. Real-time MIDI scanner & USB event loop
    // -------------------------------------------------------------------------
    loop {
        usb_stack.poll();

        // Process incoming MIDI commands from PC (Channels 3, 4, 5 control LEDs)
        process_incoming_midi(&mut usb_stack, &mut host_leds, &strand_colors);

        let pressed_keys = key_read_raw();

        // Send NoteOn/NoteOff immediately on first edge (low-latency debounce)
        process_keys(pressed_keys, &mut btn_state, &mut usb_stack);

        // Set buffer directly from host MIDI state (driven by Ableton/host PC)
        buffer.clear();
        for btn in 0..NUM_BUTTONS {
            let base_led = btn * 2;
            buffer.set_button_split(btn, host_leds[base_led], host_leds[base_led + 1]);
        }

        // Interleave USB poll() between each strand write (~0.96ms apart).
        // This guarantees macOS gets a steady ~1ms polling window during enumeration
        // while keeping LED updates smooth and responsive.
        usb_stack.poll();
        led_driver.send_strand0(&buffer);

        usb_stack.poll();
        led_driver.send_strand1(&buffer);

        usb_stack.poll();
        led_driver.send_strand2(&buffer);

        usb_stack.poll();
        led_driver.send_strand3(&buffer);

        led_driver.latch_frame();
    }
}

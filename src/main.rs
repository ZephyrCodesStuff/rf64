#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]

mod bootloader;
mod delay;
mod gpio;
mod keys;
mod led;
mod midi;
mod palette;
mod usb;

use atmega_hal::Peripherals;
use gpio::LedPins;
use keys::{key_read_raw, key_setup};
use led::{Color, LedDriver, NUM_BUTTONS, PhysicalLedBuffer, TOTAL_LEDS};
use midi::{ButtonState, process_keys};

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
const fn panic(_info: &core::panic::PanicInfo) -> ! {
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

    let mut host_leds: [Color; TOTAL_LEDS] = [Color::BLACK; TOTAL_LEDS];

    // -------------------------------------------------------------------------
    // 3. Real-time MIDI scanner & USB event loop
    // -------------------------------------------------------------------------
    loop {
        // Wait and drain all MIDI packets until the stream is idle for ~300us.
        // Ableton sends frame updates as a series of NoteOffs followed by NoteOns.
        // If we draw the LEDs in the middle of this stream, they will flicker black.
        // We ensure a full "frame" is received by waiting for a short gap in USB traffic.
        let mut idle_cycles = 0;
        loop {
            usb_stack.poll();
            let mut read_any = false;

            while let Some(packet) = usb_stack.read_packet() {
                read_any = true;

                let status = packet[1];
                let note = packet[2];
                let velocity = packet[3];
                let channel = status & 0x0F;
                let cmd = status & 0xF0;

                let is_on = (cmd == 0x90) && (velocity > 0);
                let is_off = (cmd == 0x80) || ((cmd == 0x90) && (velocity == 0));

                if (is_on || is_off)
                    && (midi::MIDI_BASENOTE..(midi::MIDI_BASENOTE + 64)).contains(&note)
                {
                    let btn = (note - midi::MIDI_BASENOTE) as usize;
                    let color = if is_on {
                        crate::palette::ABLETON_COLORS[velocity as usize]
                    } else {
                        crate::led::Color::BLACK
                    };
                    let base_led = btn * 2;
                    match channel {
                        2 => {
                            host_leds[base_led] = color;
                            host_leds[base_led + 1] = color;
                        }
                        3 => {
                            host_leds[base_led] = color;
                        }
                        4 => {
                            host_leds[base_led + 1] = color;
                        }
                        _ => {}
                    }
                }
            }

            if read_any {
                idle_cycles = 0; // reset idle counter if we got data
            } else {
                idle_cycles += 1;
                if idle_cycles > 30 {
                    break; // ~300us of idle time, stream is stable
                }
                crate::delay::delay_us(10);
            }
        }

        let pressed_keys = key_read_raw();
        process_keys(pressed_keys, &mut btn_state, &mut usb_stack);

        buffer.clear();
        for btn in 0..NUM_BUTTONS {
            let base_led = btn * 2;
            buffer.set_button_split(btn, host_leds[base_led], host_leds[base_led + 1]);
        }

        // Draw the fully stable frame. (Takes ~3.8ms total).
        // Since we disabled interrupts during WS2812, USB hardware FIFO will buffer
        // incoming packets during this time.
        led_driver.send_strand0(&buffer);
        usb_stack.poll(); // Keep USB alive between strands

        led_driver.send_strand1(&buffer);
        usb_stack.poll();

        led_driver.send_strand2(&buffer);
        usb_stack.poll();

        led_driver.send_strand3(&buffer);
        usb_stack.poll();

        led_driver.latch_frame();
    }
}

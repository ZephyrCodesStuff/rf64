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
use led::{Color, LedDriver, LEDS_PER_STRAND, TOTAL_LEDS};
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

/// Parallel bit buffer for strands 0, 2, 3 (PORTB). Placed in BSS as a `static mut`
/// so the 768 bytes are not pushed onto the call stack on every frame.
///
/// # Safety
/// This firmware is single-threaded (no interrupts touching LED state), so the
/// exclusive access pattern `fill → send` in the main loop is always race-free.
static mut PAR_BUF: led::ParallelBitBuffer = led::ParallelBitBuffer::new();

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
        let mut received_on = [false; 64];
        let mut force_draw = false;
        
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
                    
                    // Frame boundary detection: if this button already received an ON 
                    // in this burst, any new event means we've crossed into the next frame!
                    if received_on[btn] {
                        force_draw = true;
                    }
                    if is_on {
                        received_on[btn] = true;
                    }

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
                
                if force_draw {
                    break;
                }
            }

            if force_draw {
                break;
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

        // --- Dynamic Power & Brightness Limiting ---
        let mut total_sum: u32 = 0;
        let mut max_component: u8 = 0;
        for c in host_leds.iter() {
            total_sum += c.r as u32 + c.g as u32 + c.b as u32;
            if c.r > max_component { max_component = c.r; }
            if c.g > max_component { max_component = c.g; }
            if c.b > max_component { max_component = c.b; }
        }

        let power_scale = if total_sum > led::SAFE_MAX_COLOR_SUM {
            ((led::SAFE_MAX_COLOR_SUM * 256) / total_sum) as u16
        } else {
            256
        };

        let bright_scale = if max_component > led::SAFE_MAX_PIXEL_COMPONENT {
            (led::SAFE_MAX_PIXEL_COMPONENT as u16 * 256) / max_component as u16
        } else {
            256
        };

        let final_scale = if power_scale < bright_scale { power_scale } else { bright_scale };

        // Draw the fully stable frame (~1.95 ms total, down from ~3.92 ms).
        //
        // Step 1: Pre-compute 768 PORTB mid-phase masks from host_leds (no timing
        //         constraints — pure Rust, ~0.05 ms). Applies final_scale.
        // Step 2: Drive strands 0 (PB6), 2 (PB5), 3 (PB4) simultaneously via a
        //         single `out PORTB` per WS2812 phase (~0.91 ms for all three).
        // Step 3: Drive strand 1 (PC6) sequentially as before (~0.96 ms). Applies final_scale.
        //
        // USB hardware FIFO buffers incoming packets during WS2812 transmission.
        // Safety: PAR_BUF is only accessed here in the single-threaded main loop.
        let par_buf = unsafe { &mut *core::ptr::addr_of_mut!(PAR_BUF) };
        led::fill_parallel_buffer_into(par_buf, &host_leds, final_scale);

        led_driver.send_portb_parallel(par_buf); // strands 0, 2, 3 in parallel
        usb_stack.poll(); // Keep USB alive between the two transmission passes

        led_driver.send_strand1(&host_leds[LEDS_PER_STRAND..LEDS_PER_STRAND * 2], final_scale);
        usb_stack.poll();

        led_driver.latch_frame();
    }
}

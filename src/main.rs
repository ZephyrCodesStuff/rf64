#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]

mod boot_anim;
mod bootloader;
mod delay;
mod gpio;
mod keys;
mod led;
mod mcu;
mod midi;
mod palette;
mod usb;

use atmega_hal::Peripherals;
use gpio::LedPins;
use keys::{key_read_raw, key_setup};
use led::{Color, LedDriver, TOTAL_LEDS};
use mcu::init_hardware_safeguards;
use midi::{ButtonState, MidiRx, process_keys};

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // Ensure GPIO direction registers for LED strands are outputs (PB6, PB5, PB4, PC6).
    // This is safe to do even if LedPins::init() already ran — idempotent.
    unsafe {
        core::arch::asm!(
            "sbi 0x04, 6", // DDRB bit 6 (strand 0)
            "sbi 0x04, 5", // DDRB bit 5 (strand 2)
            "sbi 0x04, 4", // DDRB bit 4 (strand 3)
            "sbi 0x07, 6", // DDRC bit 6 (strand 1)
            options(nomem, nostack)
        );
    }

    // Reuse the existing PAR_BUF static (768 bytes, already in BSS).
    // send_checkerboard_direct uses zero additional stack for LED data.
    let par_buf = unsafe { &mut *core::ptr::addr_of_mut!(PAR_BUF) };
    let led_driver = LedDriver::new();
    led_driver.send_checkerboard_direct(par_buf, Color::RED);

    #[allow(clippy::empty_loop, reason = "Panic handler should never return")]
    loop {
        core::hint::spin_loop();
    }
}


/// Parallel bit buffer for strands 0, 2, 3 (PORTB).
///
/// # Safety
/// This firmware is single-threaded (no interrupts touching LED state), so the
/// exclusive access pattern `fill → send` in the main loop is always race-free.
static mut PAR_BUF: led::ParallelBitBuffer = led::ParallelBitBuffer::new();

#[atmega_hal::entry]
fn main() -> ! {
    // 0. Disable interrupts immediately! LUFA bootloader may leave them enabled, 
    //    causing immediate resets or breaking WDT disable timing.
    avr_device::interrupt::disable();

    // -------------------------------------------------------------------------
    // 1. Low-level hardware safeguards (WDT disable, bootloader check, 16 MHz, JTAG disable)
    // -------------------------------------------------------------------------
    init_hardware_safeguards();

    // -------------------------------------------------------------------------
    // 2. Initialize HAL peripherals, USB stack, key matrix & LED driver
    // -------------------------------------------------------------------------
    let dp = Peripherals::take().unwrap();
    let _led_pins = LedPins::init(&dp.PORTB, &dp.PORTC);

    // Initialize 48MHz USB PLL clock and USB bus allocator
    usb::init_usb_pll();
    let bus_allocator = usb::create_usb_bus(dp.USB_DEVICE);
    let mut usb_stack = usb::UsbMidiStack::new(&bus_allocator);

    let pins = atmega_hal::pins!(dp);
    key_setup(pins.pd7, pins.pd6, pins.pc7);

    // Give hardware (WS2812 LEDs and CD4021B shift registers) a moment to stabilize 
    // their power state before we read keys or blast LED data.
    crate::delay::delay_ms(50);

    let led_driver = LedDriver::new();
    let midi_rx = MidiRx::new();

    let initial_keys = key_read_raw();

    // Jump into DFU bootloader if Button 0 (bit 0) is held down at startup
    if bootloader::bootloader_combo_held(initial_keys) {
        // Signal bootloader entry with orange checkerboard — zero stack allocation.
        let par_buf = unsafe { &mut *core::ptr::addr_of_mut!(PAR_BUF) };
        led_driver.send_checkerboard_direct(par_buf, Color::ORANGE);

        bootloader::jump_to_bootloader();
    }

    // DEBUG: Trigger a panic if Button 1 (2nd button, bit 1) is held down at startup
    if (initial_keys & 0b10) != 0 {
        panic!("Button 1 held at startup debug panic");
    }

    // Initial LED and BTN status
    let mut btn_state = ButtonState::new();
    let mut host_leds: [Color; TOTAL_LEDS] = [Color::BLACK; TOTAL_LEDS];
    let mut dirty = true; // Whether we should redraw

    // Blackout the entire grid ONCE at boot to clear residual LEDs from a previous session
    let par_buf = unsafe { &mut *core::ptr::addr_of_mut!(PAR_BUF) };
    led_driver.render_frame(par_buf, &host_leds, &mut usb_stack);

    // Boot animation: Conway's Game of Life :)
    let mut animating = true;
    let mut life_sim = boot_anim::LifeSim::new();
    let mut life_timer = 0;

    // -------------------------------------------------------------------------
    // 3. Main Event & Frame Sync Loop
    // -------------------------------------------------------------------------
    loop {
        // A. Poll & drain incoming USB MIDI packets from DAW
        if midi_rx.drain_incoming_frame(&mut usb_stack, &mut host_leds, &mut animating) {
            dirty = true;
        }

        // B. Key matrix scanning & debounced MIDI TX
        let pressed_keys = key_read_raw();
        if pressed_keys != 0 {
            if animating {
                animating = false; // Stop boot animation if physical button is pressed
                host_leds.fill(Color::BLACK);
            }
            dirty = true;
        }
        process_keys(pressed_keys, &mut btn_state, &mut usb_stack);

        // C. Boot animation ticker (Conway's Game of Life)
        if animating {
            // Update the board every 116 ticks
            life_timer += 1;
            if life_timer >= 116 {
                life_sim.step();
                life_timer = 0;
                dirty = true;
            }

            if dirty {
                let current_state = life_sim.state();
                for btn in 0..64 {
                    let color = if (current_state >> btn) & 1 != 0 {
                        Color::WHITE
                    } else {
                        Color::BLACK
                    };
                    let base_led = btn * 2;
                    host_leds[base_led] = color;
                    host_leds[base_led + 1] = color;
                }
            }
        }

        // D. Power/Brightness Scaled Frame Transmission
        if dirty {
            let par_buf = unsafe { &mut *core::ptr::addr_of_mut!(PAR_BUF) };
            led_driver.render_frame(par_buf, &host_leds, &mut usb_stack);
            dirty = false;
        }
    }
}

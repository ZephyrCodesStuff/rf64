#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]

#[cfg(feature = "boot-anim")]
mod boot_anim;
mod bootloader;
mod buttons;
mod delay;
#[cfg(feature = "apollo")]
mod fastled;
mod gpio;
#[cfg(feature = "keyboard")]
mod keyboard;
mod led;
mod mcu;
mod midi;
mod palette;
#[cfg(feature = "apollo")]
mod sysex;
mod usb;

use buttons::{buttons_read_raw, buttons_setup};
use gpio::LedPins;
use led::{Color, LedDriver};
use mcu::init_hardware_safeguards;
use midi::{MidiRx, process_buttons};

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

/// Current RGB color for each of the 128 physical WS2812 LEDs
/// (64 buttons × 2 LEDs each, 3 bytes per LED).
static mut HOST_LEDS: [led::Color; led::TOTAL_LEDS] = [led::Color::BLACK; led::TOTAL_LEDS];

/// SysEx Parser State Machine & Buffer. In BSS to prevent stack overflow.
#[cfg(feature = "apollo")]
static mut SYSEX_PARSER: sysex::SysExParser = sysex::SysExParser::new();

/// Snake boot animation state. In BSS (~80 bytes) to keep it off main()'s stack frame.
#[cfg(feature = "boot-anim")]
static mut SNAKE_SIM: boot_anim::SnakeSim = boot_anim::SnakeSim::new();

/// Debounced button state. In BSS (~128 bytes) to keep it off main()'s stack frame.
static mut BTN_STATE: midi::ButtonState = midi::ButtonState::new();

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
    // 2. Initialize HAL peripherals, button matrix & LED driver
    // -------------------------------------------------------------------------
    let dp = unsafe { atmega_hal::Peripherals::steal() };
    let _led_pins = LedPins::init(&dp.PORTB, &dp.PORTC);

    // Initialize Timer1 for 1-second idle counting (prescaler 1024 => 15,625 Hz at 16 MHz)
    #[cfg(feature = "boot-anim")]
    dp.TC1.tccr1b().write(|w| unsafe { w.bits(0x05) });

    let pins = atmega_hal::pins!(dp);
    buttons_setup(pins.pd7, pins.pd6, pins.pc7);

    // Give hardware (WS2812 LEDs and CD4021B shift registers) a moment to stabilize
    // their power state before we read buttons or blast LED data.
    crate::delay::delay_ms(50);

    let led_driver = LedDriver::new();
    let midi_rx = MidiRx::new();

    let initial_buttons = buttons_read_raw();

    // Jump into DFU bootloader if Button 0 (bit 0) is held down at startup
    if bootloader::bootloader_combo_held(initial_buttons) {
        // Signal bootloader entry with orange checkerboard — zero stack allocation.
        let par_buf = unsafe { &mut *core::ptr::addr_of_mut!(PAR_BUF) };
        led_driver.send_checkerboard_direct(par_buf, Color::ORANGE);

        bootloader::jump_to_bootloader();
    }

    // DEBUG: Trigger a panic if Button 1 (2nd button, bit 1) is held down at startup
    #[cfg(debug_assertions)]
    if (initial_buttons & 0b10) != 0 {
        panic!("DEBUG: Button 1 held on boot, requesting panic handler.");
    }

    // 3rd button held on boot (bit 2) -> USB HID Keyboard Emulation Mode
    #[cfg(feature = "keyboard")]
    let is_keyboard_mode = (initial_buttons & 0b100) != 0;

    // Initialize 48MHz USB PLL and corresponding USB stack
    usb::init_usb_pll();

    #[cfg(feature = "keyboard")]
    if is_keyboard_mode {
        usb::init_keyboard_global(dp.USB_DEVICE);

        // Signal Keyboard mode entry with checkerboard
        let par_buf = unsafe { &mut *core::ptr::addr_of_mut!(PAR_BUF) };
        led_driver.send_checkerboard_direct(par_buf, Color::WHITE);
        crate::delay::delay_ms(1000);
    } else {
        usb::init_global(dp.USB_DEVICE);
    }

    #[cfg(not(feature = "keyboard"))]
    usb::init_global(dp.USB_DEVICE);

    // SAFETY: single-threaded; all statics are only accessed from this function.
    let host_leds = unsafe { &mut *core::ptr::addr_of_mut!(HOST_LEDS) };
    let btn_state = unsafe { &mut *core::ptr::addr_of_mut!(BTN_STATE) };
    #[cfg(feature = "boot-anim")]
    let snake_sim = unsafe { &mut *core::ptr::addr_of_mut!(SNAKE_SIM) };
    #[cfg(feature = "boot-anim")]
    snake_sim.seed(mcu::get_wdt_jitter_entropy());

    // -------------------------------------------------------------------------
    // 3. Keyboard Mode Loop (if activated on boot)
    // -------------------------------------------------------------------------
    #[cfg(feature = "keyboard")]
    if is_keyboard_mode {
        let mut prev_fn_pressed = false;

        // Render initial category background colors for all buttons (~10% brightness)
        for btn in 0..64 {
            let color = keyboard::get_button_color(btn, false, false);
            host_leds[btn * 2] = color;
            host_leds[btn * 2 + 1] = color;
        }
        let par_buf = unsafe { &mut *core::ptr::addr_of_mut!(PAR_BUF) };
        led_driver.render_frame(par_buf, host_leds);

        let mut prev_report = [0u8; 8];

        loop {
            crate::usb::poll();

            let pressed_buttons = buttons_read_raw();
            let (report, is_fn_pressed) = keyboard::build_keyboard_report(pressed_buttons);

            let fn_changed = is_fn_pressed != prev_fn_pressed;
            let report_changed = report != prev_report;

            if report_changed {
                let _ = crate::usb::send_keyboard_report(&report);
                prev_report = report;
            }

            // Update LEDs if the HID report changed OR if the FN layer toggled
            if report_changed || fn_changed {
                prev_fn_pressed = is_fn_pressed;

                // Full category color when pressed, dim category color when unpressed
                // Colors change dynamically based on active layer!
                for btn in 0..64 {
                    let is_pressed = (pressed_buttons & (1u64 << btn)) != 0;
                    let color = keyboard::get_button_color(btn, is_pressed, is_fn_pressed);
                    host_leds[btn * 2] = color;
                    host_leds[btn * 2 + 1] = color;
                }
                let par_buf = unsafe { &mut *core::ptr::addr_of_mut!(PAR_BUF) };
                led_driver.render_frame(par_buf, host_leds);
            }
        }
    }

    let mut dirty = true; // Whether we should redraw

    // Blackout the entire grid ONCE at boot to clear residual LEDs from a previous session
    let par_buf = unsafe { &mut *core::ptr::addr_of_mut!(PAR_BUF) };
    led_driver.render_frame(par_buf, host_leds);

    // Boot animation: snake game :)
    #[cfg(feature = "boot-anim")]
    let mut animating = true;
    #[cfg(not(feature = "boot-anim"))]
    let mut animating = false;

    #[cfg(feature = "boot-anim")]
    let mut last_anim_tcnt: u16 = 0;
    #[cfg(feature = "boot-anim")]
    let mut anim_substep = false;

    #[cfg(feature = "boot-anim")]
    let mut seconds_idle: u16 = 0;

    // -------------------------------------------------------------------------
    // 3. Main Event & Frame Sync Loop
    // -------------------------------------------------------------------------
    loop {
        // ALWAYS poll the USB device so it can process setup packets and enumeration
        crate::usb::poll();

        // 0. Monitor 1-second hardware timer tick (15,625 Hz) for idle timeout
        #[cfg(feature = "boot-anim")]
        {
            let tcnt = dp.TC1.tcnt1().read().bits();
            if tcnt >= 15625 {
                dp.TC1.tcnt1().write(|w| unsafe { w.bits(tcnt - 15625) });
                seconds_idle += 1;

                if seconds_idle >= 256 && !animating {
                    animating = true;
                    snake_sim.reset();
                    dirty = true;
                    seconds_idle = 0;
                    last_anim_tcnt = tcnt;
                    anim_substep = false;
                }
            }
        }

        // A. Poll & drain incoming USB MIDI packets from DAW
        #[cfg(feature = "apollo")]
        let sysex_parser_opt = Some(unsafe { &mut *core::ptr::addr_of_mut!(SYSEX_PARSER) });
        #[cfg(not(feature = "apollo"))]
        let sysex_parser_opt: Option<&mut ()> = None;

        let midi = midi_rx.drain_incoming_frame(host_leds, &mut animating, sysex_parser_opt);

        // Midi dirty
        if midi.0 {
            dirty = true;
        }

        // Midi activity
        #[cfg(feature = "boot-anim")]
        if midi.1 {
            seconds_idle = 0;
            dp.TC1.tcnt1().write(|w| unsafe { w.bits(0) });
        }

        // B. Button matrix scanning & debounced MIDI TX
        let pressed_buttons = buttons_read_raw();
        if pressed_buttons != 0 {
            // Reset idle timer on physical button press
            #[cfg(feature = "boot-anim")]
            {
                seconds_idle = 0;
                dp.TC1.tcnt1().write(|w| unsafe { w.bits(0) });
            }

            if animating {
                animating = false; // Stop boot animation if physical button is pressed
                host_leds.fill(Color::BLACK);
            }
            dirty = true;
        }
        process_buttons(pressed_buttons, btn_state);

        // C. Boot animation ticker (snake game)
        //
        // Hardware-paced via Timer1 (15,625 Hz, 64us per tick):
        //   1172 ticks (~75ms)  → half_step(): preview entry/exit LEDs
        //   2344 ticks (~150ms) → step():      commit move; lit new head
        #[cfg(feature = "boot-anim")]
        if animating {
            let tcnt = dp.TC1.tcnt1().read().bits();
            let elapsed = if tcnt >= last_anim_tcnt {
                tcnt - last_anim_tcnt
            } else {
                tcnt + 15625 - last_anim_tcnt
            };

            if !anim_substep && elapsed >= 1172 {
                snake_sim.half_step();
                snake_sim.fill_leds(host_leds);
                dirty = true;
                anim_substep = true;
            } else if anim_substep && elapsed >= 2344 {
                snake_sim.step();
                snake_sim.fill_leds(host_leds);
                dirty = true;
                anim_substep = false;
                last_anim_tcnt = tcnt;
            }
        }

        // D. Power/Brightness Scaled Frame Transmission
        if dirty {
            let par_buf = unsafe { &mut *core::ptr::addr_of_mut!(PAR_BUF) };
            led_driver.render_frame(par_buf, host_leds);
            dirty = false;
        }
    }
}

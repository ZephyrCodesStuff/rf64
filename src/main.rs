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

use buttons::{ButtonMatrix, Debouncer};
use gpio::LedPins;
use led::{Color, LedDriver};
use midi::{MidiRx, send_button_events};

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

/// Debounced button state. In BSS (~144 bytes) to keep it off main()'s stack frame.
static mut DEBOUNCER: Debouncer = Debouncer::new();

#[atmega_hal::entry]
fn main() -> ! {
    // -------------------------------------------------------------------------
    // 0. Disable interrupts immediately! LUFA bootloader may leave them enabled,
    //    causing immediate resets or breaking WDT disable timing.
    // -------------------------------------------------------------------------
    avr_device::interrupt::disable();

    // -------------------------------------------------------------------------
    // 1. Low-level hardware safeguards (WDT disable, bootloader check, 16 MHz, JTAG disable)
    // -------------------------------------------------------------------------
    let dp = atmega_hal::Peripherals::take().expect("could not take peripherals");

    // Check if the user has requested to jump to bootloader
    bootloader::check_bootloader_requested(&dp);

    // Set clock to max speed (16 MHz)
    //
    // Datasheet specifies this to use the same mechanism as WDT: enable then set within next 4 clock cycles
    dp.CPU.clkpr().write(|w| w.clkpce().set_bit());
    dp.CPU.clkpr().write(|w| w.clkps().val_0x00());

    // Disable JTAG to free GPIO ports C and F
    //
    // Doing this twice is _intended_: it is a datasheet security measure against glitches
    dp.JTAG.mcucr().write(|w| w.jtd().set_bit());
    dp.JTAG.mcucr().write(|w| w.jtd().set_bit());

    // -------------------------------------------------------------------------
    // 2. Initialize HAL peripherals, button matrix & LED driver
    // -------------------------------------------------------------------------
    LedPins::setup(&dp.PORTB, &dp.PORTC);

    // Initialize Timer1 as a free-running 16-bit monotonic counter
    // Prescaler 1024 => 15,625 Hz at 16 MHz (1 tick = 64 µs, wraps every ~4.19s)
    dp.TC1.tccr1b().write(|w| unsafe { w.bits(0x05) });

    let pins = atmega_hal::pins!(dp);
    let mut button_matrix = ButtonMatrix::new(pins.pd7, pins.pd6, pins.pc7);

    // Give hardware (WS2812 LEDs and CD4021B shift registers) a moment to stabilize
    // their power state before we read buttons or blast LED data.
    crate::delay::delay_ms(50);

    let led_driver = LedDriver::new();
    let midi_rx = MidiRx::new();

    let initial_buttons = button_matrix.read_raw();

    // Jump into DFU bootloader if Button 0 (bit 0) is held down at startup
    if bootloader::bootloader_combo_held(initial_buttons) {
        // Signal bootloader entry with orange checkerboard — zero stack allocation.
        let par_buf = unsafe { &mut *core::ptr::addr_of_mut!(PAR_BUF) };
        led_driver.send_checkerboard_direct(par_buf, Color::ORANGE);

        // SAFETY: we aren't touching pins or buttons, only the watchdog
        let p = unsafe { atmega_hal::Peripherals::steal() };
        bootloader::request_bootloader(&p);
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
    let debouncer = unsafe { &mut *core::ptr::addr_of_mut!(DEBOUNCER) };
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

            let now_tick = dp.TC1.tcnt1().read().bits();
            let raw_buttons = button_matrix.read_raw();
            let _ = debouncer.update(raw_buttons, now_tick);

            let (report, is_fn_pressed) = keyboard::build_keyboard_report(debouncer.state);

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
                    let is_pressed = (debouncer.state & (1u64 << btn)) != 0;
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
    let mut last_second_tick: u16 = 0;
    #[cfg(feature = "boot-anim")]
    let mut last_anim_tick: u16 = 0;
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

        let now_tick = dp.TC1.tcnt1().read().bits();

        // 0. Monitor 1-second hardware timer tick (15,625 Hz) for idle timeout
        #[cfg(feature = "boot-anim")]
        {
            let elapsed_sec = now_tick.wrapping_sub(last_second_tick);
            if elapsed_sec >= 15625 {
                last_second_tick = last_second_tick.wrapping_add(15625);
                seconds_idle = seconds_idle.saturating_add(1);

                if seconds_idle >= 256 && !animating {
                    animating = true;
                    snake_sim.reset();
                    dirty = true;
                    seconds_idle = 0;
                    last_anim_tick = now_tick;
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

        // B. Button matrix scanning & time-debounced MIDI TX
        let raw_buttons = button_matrix.read_raw();
        let (pressed_edges, released_edges) = debouncer.update(raw_buttons, now_tick);
        let button_activity = (pressed_edges | released_edges) != 0;

        if button_activity {
            send_button_events(pressed_edges, released_edges);
        }

        // Reset idle timer and stop boot animation if physical button or MIDI received
        #[cfg(feature = "boot-anim")]
        if midi.1 || button_activity {
            seconds_idle = 0;
            if animating {
                animating = false; // Stop boot animation if button is pressed or MIDI received
                host_leds.fill(Color::BLACK);
                dirty = true;
            }
        }

        // C. Boot animation ticker (snake game)
        //
        // Hardware-paced via Timer1 (15,625 Hz, 64us per tick):
        //   1172 ticks (~75ms)  → half_step(): preview entry/exit LEDs
        //   2344 ticks (~150ms) → step():      commit move; lit new head
        #[cfg(feature = "boot-anim")]
        if animating {
            let elapsed = now_tick.wrapping_sub(last_anim_tick);

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
                last_anim_tick = now_tick;
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

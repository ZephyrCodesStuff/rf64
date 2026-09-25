#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]

/*
todo (lots of stuff)

refactor main while loop (it's a damn mess)
try implementing middleware/hal separation
implement testing for the logic (separate logic as much as possible from hw state)
reduce local variable hell
*/

mod boot;
mod midi;
mod usb;

#[cfg(feature = "instrumentation")]
mod instrumentation;

mod buttons;
mod delay;
mod gpio;

mod led;
mod rng;

use buttons::{ButtonMatrix, Debouncer};
use gpio::LedPins;
use led::Color;
use midi::io::MidiRx;

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
    led::send_checkerboard_direct(par_buf, Color::RED);

    #[allow(clippy::empty_loop, reason = "panic handler should never return")]
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
static mut HOST_LEDS: [Color; led::TOTAL_LEDS] = [Color::BLACK; led::TOTAL_LEDS];

/// SysEx Parser State Machine & Buffer. In BSS to prevent stack overflow.
#[cfg(feature = "apollo")]
static mut SYSEX_PARSER: midi::sysex::SysExParser = midi::sysex::SysExParser::new();

/// Idle boot animation controller. In BSS (~80 bytes) to keep it off main()'s stack frame.
#[cfg(feature = "boot-anim")]
static mut IDLE_ANIM: boot::snake::IdleAnimation = boot::snake::IdleAnimation::new();

/// Debounced button state. In BSS (~144 bytes) to keep it off main()'s stack frame.
static mut DEBOUNCER: Debouncer = Debouncer::new();

/// Optional telemetry state stays in BSS instead of growing main's stack frame.
#[cfg(feature = "instrumentation")]
static mut METRICS: instrumentation::Metrics = instrumentation::Metrics::new();

/// Limit LED frame updates to about 300 Hz (52 × 64 µs timer ticks).
const LED_FRAME_INTERVAL_TICKS: u16 = 52;

pub struct Board {
    pub button_matrix: ButtonMatrix,
    pub tc1: atmega_hal::pac::TC1,
    #[cfg(feature = "keyboard")]
    pub is_keyboard_mode: bool,
}

impl Board {
    #[inline(always)]
    pub fn now_tick(&self) -> u16 {
        self.tc1.tcnt1().read().bits()
    }
}

fn setup() -> Board {
    // -------------------------------------------------------------------------
    // 0. Disable interrupts immediately! LUFA bootloader may leave them enabled,
    //    causing immediate resets or breaking WDT disable timing.
    // -------------------------------------------------------------------------
    avr_device::interrupt::disable();

    let dp = atmega_hal::Peripherals::take().expect("could not take peripherals");

    // -------------------------------------------------------------------------
    // 1. Low-level hardware safeguards (WDT disable, bootloader check, 16 MHz, JTAG disable)
    // -------------------------------------------------------------------------
    boot::bootloader::check_bootloader_requested(&dp);

    // Set clock to max speed (16 MHz)
    // Datasheet specifies this to use the same mechanism as WDT: enable then set within next 4 clock cycles
    dp.CPU.clkpr().write(|w| w.clkpce().set_bit());
    dp.CPU.clkpr().write(|w| w.clkps().val_0x00());

    // Disable JTAG to free GPIO ports C and F
    // Doing this twice is _intended_: it is a datasheet security measure against glitches
    dp.JTAG.mcucr().write(|w| w.jtd().set_bit());
    dp.JTAG.mcucr().write(|w| w.jtd().set_bit());

    // -------------------------------------------------------------------------
    // 2. Initialize HAL peripherals, button matrix & LED driver
    // -------------------------------------------------------------------------
    LedPins::setup(&dp.PORTB, &dp.PORTC);

    #[cfg(feature = "boot-anim")]
    {
        let idle_anim = unsafe { &mut *core::ptr::addr_of_mut!(IDLE_ANIM) };
        idle_anim.seed(rng::get_wdt_jitter_entropy(&dp));
    }

    // Initialize Timer1 as a free-running 16-bit monotonic counter
    // Prescaler 1024 => 15,625 Hz at 16 MHz (1 tick = 64 µs, wraps every ~4.19s)
    dp.TC1.tccr1b().write(|w| unsafe { w.bits(0x05) });

    let tc1 = dp.TC1;
    let usb_device = dp.USB_DEVICE;

    let pins = atmega_hal::pins!(dp);
    let mut button_matrix = ButtonMatrix::new(pins.pd7, pins.pd6, pins.pc7);

    // Give hardware (WS2812 LEDs and CD4021B shift registers) a moment to stabilize
    // their power state before we read buttons or blast LED data.
    delay::delay_ms(50);

    let initial_buttons = button_matrix.read_raw();

    // Jump into DFU bootloader if Button 0 (bit 0) is held down at startup
    if boot::bootloader::bootloader_combo_held(initial_buttons) {
        // Signal bootloader entry with orange checkerboard — zero stack allocation.
        let par_buf = unsafe { &mut *core::ptr::addr_of_mut!(PAR_BUF) };
        led::send_checkerboard_direct(par_buf, Color::ORANGE);

        // SAFETY: we aren't touching pins or buttons, only the watchdog
        let p = unsafe { atmega_hal::Peripherals::steal() };
        boot::bootloader::request_bootloader(&p);
    }

    // DEBUG: Trigger a panic if Button 1 (2nd button, bit 1) is held down at startup
    #[cfg(debug_assertions)]
    if initial_buttons.is_set(1) {
        panic!("DEBUG: Button 1 held on boot, requesting panic handler.");
    }

    // 3rd button held on boot (bit 2) -> USB HID Keyboard Emulation Mode
    #[cfg(feature = "keyboard")]
    let is_keyboard_mode = usb::keyboard::keyboard_combo_held(initial_buttons);

    // Initialize 48MHz USB PLL and corresponding USB stack
    usb::init_usb_pll();

    #[cfg(feature = "keyboard")]
    if is_keyboard_mode {
        usb::init_keyboard_global(usb_device);

        // Signal Keyboard mode entry with checkerboard
        let par_buf = unsafe { &mut *core::ptr::addr_of_mut!(PAR_BUF) };
        led::send_checkerboard_direct(par_buf, Color::WHITE);
        crate::delay::delay_ms(1000);
    } else {
        usb::init_global(usb_device);
    }

    #[cfg(not(feature = "keyboard"))]
    usb::init_global(usb_device);

    Board {
        button_matrix,
        tc1,
        #[cfg(feature = "keyboard")]
        is_keyboard_mode,
    }
}

#[atmega_hal::entry]
fn main() -> ! {
    let mut board = setup();
    let mut midi_rx = MidiRx::new();

    // SAFETY: single-threaded; all statics are only accessed from this function.
    let host_leds = unsafe { &mut *core::ptr::addr_of_mut!(HOST_LEDS) };
    let debouncer = unsafe { &mut *core::ptr::addr_of_mut!(DEBOUNCER) };
    let par_buf = unsafe { &mut *core::ptr::addr_of_mut!(PAR_BUF) };

    #[cfg(feature = "apollo")]
    let sysex_parser = unsafe { &mut *core::ptr::addr_of_mut!(SYSEX_PARSER) };

    #[cfg(feature = "boot-anim")]
    let idle_anim = unsafe { &mut *core::ptr::addr_of_mut!(IDLE_ANIM) };

    // -------------------------------------------------------------------------
    // 3. Keyboard Mode Loop (if activated on boot)
    // -------------------------------------------------------------------------
    #[cfg(feature = "keyboard")]
    if board.is_keyboard_mode {
        usb::keyboard::run_keyboard_mode(
            &board.tc1,
            &mut board.button_matrix,
            debouncer,
            host_leds,
            par_buf,
        );
    }

    let mut dirty = false; // Whether the LED buffer has changed since the last render

    // Blackout the entire grid ONCE at boot to clear residual LEDs from a previous session
    led::render_frame(par_buf, host_leds);
    let mut last_frame_tick = board.now_tick();

    // -------------------------------------------------------------------------
    // 4. Main Event & Frame Sync Loop
    // -------------------------------------------------------------------------
    loop {
        // ALWAYS poll the USB device so it can process setup packets and enumeration
        usb::poll();

        let now_tick = board.now_tick();

        // A. Poll & drain incoming USB MIDI packets from DAW
        #[cfg(feature = "instrumentation")]
        let midi_started_tick = now_tick;
        let midi = midi_rx.drain_stable_batch(
            host_leds,
            #[cfg(feature = "boot-anim")]
            &mut idle_anim.animating,
            #[cfg(not(feature = "boot-anim"))]
            &mut false,
            #[cfg(feature = "apollo")]
            Some(&mut *sysex_parser),
            #[cfg(not(feature = "apollo"))]
            None,
            || board.now_tick(),
        );
        #[cfg(feature = "instrumentation")]
        {
            let elapsed = board.now_tick().wrapping_sub(midi_started_tick);
            let metrics = unsafe { &mut *core::ptr::addr_of_mut!(METRICS) };
            instrumentation::TraceSink::event(
                metrics,
                instrumentation::TraceEvent::MidiBatch {
                    packet_count: midi.packets,
                    duration_ticks: elapsed,
                },
            );
        }

        // Midi dirty
        dirty |= midi.dirty;

        // B. Button matrix scanning & time-debounced MIDI TX
        let (pressed_edges, released_edges) =
            debouncer.update(board.button_matrix.read_raw(), now_tick);
        let button_activity = pressed_edges.any() || released_edges.any();

        if button_activity {
            midi::io::send_button_events(pressed_edges, released_edges);
        }

        // Reset idle timer and stop boot animation if physical button or MIDI received
        #[cfg(feature = "boot-anim")]
        if (midi.activity || button_activity) && idle_anim.notify_activity(host_leds) {
            dirty = true;
        }

        // Advance idle timer and snake animation
        #[cfg(feature = "boot-anim")]
        if idle_anim.tick(now_tick, host_leds) {
            dirty = true;
        }

        // D. Power/Brightness Scaled Frame Transmission, capped near 300 FPS.
        // Multiple MIDI changes between slots collapse into the newest LED state.
        if dirty && now_tick.wrapping_sub(last_frame_tick) >= LED_FRAME_INTERVAL_TICKS {
            #[cfg(feature = "instrumentation")]
            let render_started_tick = board.now_tick();
            led::render_frame(par_buf, host_leds);
            #[cfg(feature = "instrumentation")]
            {
                let elapsed = board.now_tick().wrapping_sub(render_started_tick);
                let metrics = unsafe { &mut *core::ptr::addr_of_mut!(METRICS) };
                instrumentation::TraceSink::event(
                    metrics,
                    instrumentation::TraceEvent::RenderFrame {
                        duration_ticks: elapsed,
                    },
                );
            }
            dirty = false;
            last_frame_tick = now_tick;
        }

        #[cfg(feature = "instrumentation")]
        {
            let metrics = unsafe { &mut *core::ptr::addr_of_mut!(METRICS) };
            metrics.report_if_due(now_tick, &mut instrumentation::MidiSysExTransport);
        }
    }
}

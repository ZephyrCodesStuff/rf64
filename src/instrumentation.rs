//! Low-overhead runtime measurements and pluggable telemetry transports.
//!
//! The measurement core only stores counters. A transport is invoked at the
//! report interval, outside the cycle-sensitive LED bitstream.
//!
//! SysEx body layout (all multi-byte values are little-endian 7-bit chunks):
//! `7D 'R' 'F' version sequence frames render_ticks_total render_ticks_max`
//! `midi_packets midi_batches max_batch_packets max_batch_ticks checksum`.
//! Tick values use the Timer1 64 µs unit; frame and MIDI totals cover one
//! approximately one-second reporting interval.

/*
todo

use a proper tracing framework for embedded no-alloc no-std envs instead of our own
simply implement a midi sysex exporter (or better yet, something via usb?) in the trait for exporting
*/

const REPORT_INTERVAL_TICKS: u16 = 15_625; // 1 second at 64 µs per Timer1 tick
const REPORT_LEN: usize = 20;

/// A compact interval summary, represented as a 7-bit-safe MIDI SysEx message.
#[derive(Clone, Copy)]
pub struct Report {
    pub sequence: u8,
    pub frames: u16,
    pub render_ticks_total: u16,
    pub render_ticks_max: u8,
    pub midi_packets: u16,
    pub midi_batches: u16,
    pub midi_batch_packets_max: u16,
    pub midi_batch_ticks_max: u8,
}

/// Pluggable output for interval reports. Implementations should avoid doing
/// work while LED data is being transmitted.
pub trait ReportTransport {
    fn send(&mut self, report: &Report);
}

/// Typed events emitted by the firmware. Call sites stay independent of how
/// events are aggregated or transported.
#[derive(Clone, Copy)]
pub enum TraceEvent {
    RenderFrame {
        duration_ticks: u16,
    },
    MidiBatch {
        packet_count: u16,
        duration_ticks: u16,
    },
}

/// No-allocation event sink, suitable for the single-threaded AVR firmware.
pub trait TraceSink {
    fn event(&mut self, event: TraceEvent);
}

/// MIDI SysEx transport. Message body: `7D 52 46 01`, sequence, then 14-bit
/// counters and 7-bit maxima, followed by a 7-bit additive checksum.
pub struct MidiSysExTransport;

impl ReportTransport for MidiSysExTransport {
    fn send(&mut self, report: &Report) {
        let mut bytes = [0u8; REPORT_LEN];
        bytes[0] = 0xF0;
        bytes[1] = 0x7D; // Educational / non-commercial manufacturer ID
        bytes[2] = 0x52; // 'R'
        bytes[3] = 0x46; // 'F'
        bytes[4] = 0x01; // Telemetry protocol version
        bytes[5] = report.sequence & 0x7F;
        put_u14(&mut bytes, 6, report.frames);
        put_u14(&mut bytes, 8, report.render_ticks_total);
        bytes[10] = report.render_ticks_max & 0x7F;
        put_u14(&mut bytes, 11, report.midi_packets);
        put_u14(&mut bytes, 13, report.midi_batches);
        put_u14(&mut bytes, 15, report.midi_batch_packets_max);
        bytes[17] = report.midi_batch_ticks_max & 0x7F;
        bytes[18] = bytes[1..18]
            .iter()
            .fold(0u8, |sum, byte| sum.wrapping_add(*byte))
            & 0x7F;
        bytes[19] = 0xF7;

        crate::usb::midi::send_sysex(&bytes);
    }
}

/// Collects a one-second measurement window. Timer-derived durations are in
/// 64 µs ticks; the LED waveform itself is never instrumented internally.
pub struct Metrics {
    sequence: u8,
    last_report_tick: u16,
    clock_started: bool,
    frames: u16,
    render_ticks_total: u16,
    render_ticks_max: u8,
    midi_packets: u16,
    midi_batches: u16,
    midi_batch_packets_max: u16,
    midi_batch_ticks_max: u8,
}

impl Metrics {
    pub const fn new() -> Self {
        Self {
            sequence: 0,
            last_report_tick: 0,
            clock_started: false,
            frames: 0,
            render_ticks_total: 0,
            render_ticks_max: 0,
            midi_packets: 0,
            midi_batches: 0,
            midi_batch_packets_max: 0,
            midi_batch_ticks_max: 0,
        }
    }

    pub fn report_if_due<T: ReportTransport>(&mut self, now_tick: u16, transport: &mut T) {
        if !self.clock_started {
            self.last_report_tick = now_tick;
            self.clock_started = true;
            return;
        }
        if now_tick.wrapping_sub(self.last_report_tick) < REPORT_INTERVAL_TICKS {
            return;
        }

        let report = Report {
            sequence: self.sequence,
            frames: self.frames,
            render_ticks_total: self.render_ticks_total,
            render_ticks_max: self.render_ticks_max,
            midi_packets: self.midi_packets,
            midi_batches: self.midi_batches,
            midi_batch_packets_max: self.midi_batch_packets_max,
            midi_batch_ticks_max: self.midi_batch_ticks_max,
        };
        transport.send(&report);

        self.sequence = self.sequence.wrapping_add(1) & 0x7F;
        self.last_report_tick = now_tick;
        self.frames = 0;
        self.render_ticks_total = 0;
        self.render_ticks_max = 0;
        self.midi_packets = 0;
        self.midi_batches = 0;
        self.midi_batch_packets_max = 0;
        self.midi_batch_ticks_max = 0;
    }
}

impl TraceSink for Metrics {
    fn event(&mut self, event: TraceEvent) {
        match event {
            TraceEvent::RenderFrame { duration_ticks } => {
                self.frames = self.frames.saturating_add(1);
                self.render_ticks_total = self.render_ticks_total.saturating_add(duration_ticks);
                self.render_ticks_max = self.render_ticks_max.max(duration_ticks.min(127) as u8);
            }
            TraceEvent::MidiBatch {
                packet_count,
                duration_ticks,
            } => {
                self.midi_packets = self.midi_packets.saturating_add(packet_count);
                self.midi_batches = self.midi_batches.saturating_add(1);
                self.midi_batch_packets_max = self.midi_batch_packets_max.max(packet_count);
                self.midi_batch_ticks_max =
                    self.midi_batch_ticks_max.max(duration_ticks.min(127) as u8);
            }
        }
    }
}

const fn put_u14(bytes: &mut [u8; REPORT_LEN], at: usize, value: u16) {
    bytes[at] = value as u8 & 0x7F;
    bytes[at + 1] = (value >> 7) as u8 & 0x7F;
}

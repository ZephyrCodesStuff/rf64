//! Snake boot animation for the MIDI Fighter 64.
//!
//! ## Pathfinding Algorithm: Shortcut Hamiltonian AI
//!
//! The snake follows a guaranteed 8×8 closed Hamiltonian cycle with dynamic
//! shortcutting:
//!   - When the apple is in sight, the snake takes direct rectangular paths
//!     toward it across the grid.
//!   - A shortcut move is only taken if `cycle_dist(head, next) < cycle_dist(head, tail)`.
//!     This rule guarantees the snake NEVER cuts off its tail or traps itself.
//!   - If no shortcut is safe, the snake safely falls back to the underlying
//!     Hamiltonian cycle.
//!
//! ## Layout & LED Smoothing
//!
//! - Button mapping uses Ableton drum rack layout (Left 4 cols = 0..31, Right 4 cols = 32..63).
//! - Sub-cell LED smoothing: half-step (tick 16) lights the entry LED of the incoming cell;
//!   full-step (tick 32) commits the move.
//! - Progressive apple eating: entry LED flips to head green while exit LED remains apple red.

use crate::led::{Color, TOTAL_LEDS};

// ── Color palette ─────────────────────────────────────────────────────────────

/// Snake head — brightest green.
const COLOR_HEAD: Color = Color::new(0, 210, 0);
/// Snake body, odd-indexed segments from head — medium green.
const COLOR_BODY_LIGHT: Color = Color::new(0, 100, 0);
/// Snake body, even-indexed segments from head — dark green.
const COLOR_BODY_DARK: Color = Color::new(0, 35, 0);
/// Apple — red, both LEDs.
const COLOR_APPLE: Color = Color::new(220, 0, 0);

// ── Direction ─────────────────────────────────────────────────────────────────

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Dir {
    Right,
    Left,
    Up,
    Down,
}

// ── Layout & Hamiltonian cycle mapping ───────────────────────────────────────

/// Map spatial grid cell `(row, col)` to physical button index `0..63`.
#[inline(always)]
const fn cell_to_btn(row: u8, col: u8) -> usize {
    let half_offset = if col >= 4 { 32 } else { 0 };
    let c = (col & 3) as usize;
    half_offset + (row as usize * 4) + c
}

const POS_TO_CELL_LUT: [u8; 64] = {
    let mut lut = [0; 64];
    let mut pos = 0;
    while pos < 64 {
        let (r, c) = match pos {
            0..=7 => (0, pos),
            8..=14 => (1, 7 - (pos - 8)),
            15..=21 => (2, 1 + (pos - 15)),
            22..=28 => (3, 7 - (pos - 22)),
            29..=35 => (4, 1 + (pos - 29)),
            36..=42 => (5, 7 - (pos - 36)),
            43..=49 => (6, 1 + (pos - 43)),
            50..=57 => (7, 7 - (pos - 50)),
            58..=63 => (6 - (pos - 58), 0),
            _ => (0, 0),
        };
        lut[pos as usize] = (r << 3) | c;
        pos += 1;
    }
    lut
};

const CELL_TO_POS_LUT: [u8; 64] = {
    let mut lut = [0; 64];
    let mut r = 0;
    while r < 8 {
        let mut c = 0;
        while c < 8 {
            let pos = match (r, c) {
                (0, c) => c,
                (1, 0) => 63,
                (1, c) => 8 + (7 - c),
                (2, 0) => 62,
                (2, c) => 15 + (c - 1),
                (3, 0) => 61,
                (3, c) => 22 + (7 - c),
                (4, 0) => 60,
                (4, c) => 29 + (c - 1),
                (5, 0) => 59,
                (5, c) => 36 + (7 - c),
                (6, 0) => 58,
                (6, c) => 43 + (c - 1),
                (7, c) => 50 + (7 - c),
                _ => 0,
            };
            lut[((r << 3) | c) as usize] = pos;
            c += 1;
        }
        r += 1;
    }
    lut
};

#[inline(always)]
const fn pos_to_cell(pos: u8) -> (u8, u8) {
    let val = POS_TO_CELL_LUT[(pos & 63) as usize];
    (val >> 3, val & 7)
}

#[inline(always)]
const fn cell_to_pos(row: u8, col: u8) -> u8 {
    CELL_TO_POS_LUT[(((row & 7) << 3) | (col & 7)) as usize]
}

/// Forward distance between two positions along the Hamiltonian cycle (0–63).
#[inline(always)]
const fn cycle_dist(from: u8, to: u8) -> u8 {
    (to + 64 - from) & 63
}

/// Manhattan grid distance.
#[inline(always)]
const fn grid_dist(pos1: u8, pos2: u8) -> u8 {
    let (r1, c1) = pos_to_cell(pos1);
    let (r2, c2) = pos_to_cell(pos2);
    r1.abs_diff(r2) + c1.abs_diff(c2)
}

/// Movement direction from `from_pos` to `to_pos`.
#[inline(always)]
const fn get_dir(from_pos: u8, to_pos: u8) -> Dir {
    let (fr, fc) = pos_to_cell(from_pos);
    let (tr, tc) = pos_to_cell(to_pos);

    if tc == (fc + 1) & 7 {
        Dir::Right
    } else if tc == (fc + 7) & 7 {
        Dir::Left
    } else if tr == (fr + 1) & 7 {
        Dir::Up
    } else {
        Dir::Down
    }
}

// ── 16-bit Linear Congruential Generator ─────────────────────────────────────

#[inline(always)]
const fn next_rand(s: u16) -> u16 {
    s.wrapping_mul(25173).wrapping_add(13849)
}

// ── Tuning constants ──────────────────────────────────────────────────────────

const INIT_LEN: u8 = 3;
const INIT_APPLE_PATH: u8 = 27;
const RESTART_LEN: u8 = 60;
const PAUSE_TICKS: u8 = 5;

// ── SnakeSim ──────────────────────────────────────────────────────────────────

/// AI-driven snake game simulation using Shortcut Hamiltonian pathfinding.
pub struct SnakeSim {
    /// Ring buffer storing cycle position indices (0..63) for active segments.
    segs: [u8; 64],
    /// Index in `segs` of current head.
    head_idx: usize,
    /// Active snake length.
    len: u8,
    /// Apple position on cycle (0..63).
    apple_path: u8,
    lfsr: u16,
    step_count: u16,
    should_grow: bool,
    pause_ticks: u8,
    sub_step: bool,
    next_head_path: u8,
    next_dir: Dir,
    tail_path: u8,
    tail_dir: Dir,
    tail_removing: bool,
}

impl SnakeSim {
    pub const fn new() -> Self {
        let mut segs = [0u8; 64];
        segs[0] = 0; // (0,0)
        segs[1] = 1; // (0,1)
        segs[2] = 2; // (0,2)
        Self {
            segs,
            head_idx: 2,
            len: INIT_LEN,
            apple_path: INIT_APPLE_PATH,
            lfsr: 0xACE1,
            step_count: 0,
            should_grow: false,
            pause_ticks: 0,
            sub_step: false,
            next_head_path: 3,
            next_dir: Dir::Right,
            tail_path: 0,
            tail_dir: Dir::Right,
            tail_removing: false,
        }
    }

    /// Check if cycle position `pos` is occupied by any active body segment.
    fn is_pos_occupied(&self, pos: u8) -> bool {
        let check_len = if self.should_grow {
            self.len as usize
        } else {
            self.len as usize - 1
        };
        for i in 0..check_len {
            let idx = (self.head_idx + 64 - i) & 63;
            if self.segs[idx] == pos {
                return true;
            }
        }
        false
    }

    /// Select the best move (targeting apple) that is guaranteed safe from self-trapping.
    fn find_best_move(&self) -> u8 {
        let head_pos = self.segs[self.head_idx];
        let (hr, hc) = pos_to_cell(head_pos);

        let tail_idx = (self.head_idx + 64 - (self.len as usize - 1)) & 63;
        let tail_pos = self.segs[tail_idx];

        // 4 orthogonal neighbors on 8x8 grid (NO wrap-around)
        let mut neighbors = [0u8; 4];
        let mut n_count = 0;
        if hc < 7 {
            neighbors[n_count] = cell_to_pos(hr, hc + 1);
            n_count += 1;
        }
        if hc > 0 {
            neighbors[n_count] = cell_to_pos(hr, hc - 1);
            n_count += 1;
        }
        if hr < 7 {
            neighbors[n_count] = cell_to_pos(hr + 1, hc);
            n_count += 1;
        }
        if hr > 0 {
            neighbors[n_count] = cell_to_pos(hr - 1, hc);
            n_count += 1;
        }

        let mut best_pos = (head_pos + 1) & 63; // Default cycle step
        let mut min_g_dist = u8::MAX;
        let mut min_c_dist = u8::MAX;

        for n_pos in neighbors.iter().take(n_count) {
            let n_pos = *n_pos;

            if self.is_pos_occupied(n_pos) {
                continue;
            }

            let is_default = n_pos == (head_pos + 1) & 63;
            let is_safe_shortcut = cycle_dist(head_pos, n_pos) < cycle_dist(head_pos, tail_pos);
            let does_not_skip_apple =
                cycle_dist(head_pos, n_pos) <= cycle_dist(head_pos, self.apple_path);

            if !is_default && !(is_safe_shortcut && does_not_skip_apple) {
                continue;
            }

            // Score: primary = Manhattan grid distance, secondary = cycle distance
            let g_dist = grid_dist(n_pos, self.apple_path);
            let c_dist = cycle_dist(n_pos, self.apple_path);

            let is_better = g_dist < min_g_dist || (g_dist == min_g_dist && c_dist < min_c_dist);

            if is_better {
                min_g_dist = g_dist;
                min_c_dist = c_dist;
                best_pos = n_pos;
            }
        }

        best_pos
    }

    pub fn seed(&mut self, seed: u16) {
        self.lfsr = if seed == 0 { 0xACE1 } else { seed };
        self.place_apple();
    }

    fn place_apple(&mut self) {
        self.lfsr = next_rand(self.lfsr);
        let start_candidate = (self.lfsr & 63) as u8;
        let mut candidate = start_candidate;
        for _ in 0..64u8 {
            if !self.is_pos_occupied(candidate) {
                self.apple_path = candidate;
                return;
            }
            candidate = (candidate + 1) & 63;
        }
        self.pause_ticks = PAUSE_TICKS;
    }

    pub fn reset(&mut self) {
        self.segs = [0u8; 64];
        self.segs[0] = 0;
        self.segs[1] = 1;
        self.segs[2] = 2;
        self.head_idx = 2;
        self.len = INIT_LEN;
        self.step_count = 0;
        self.should_grow = false;
        self.pause_ticks = 0;
        self.sub_step = false;
        for _ in 0..11u8 {
            self.lfsr = next_rand(self.lfsr);
        }
        self.place_apple();
    }

    /// Half-step preview for smooth sub-cell transition (tick 16).
    pub fn half_step(&mut self) {
        if self.pause_ticks > 0 {
            return;
        }

        let next_pos = self.find_best_move();
        let head_pos = self.segs[self.head_idx];
        self.next_head_path = next_pos;
        self.next_dir = get_dir(head_pos, next_pos);

        self.tail_removing = !self.should_grow;
        if self.tail_removing {
            let tail_idx = (self.head_idx + 64 - (self.len as usize - 1)) & 63;
            let prev_tail_idx = (self.head_idx + 64 - (self.len as usize - 2)) & 63;
            self.tail_path = self.segs[tail_idx];
            self.tail_dir = get_dir(self.segs[prev_tail_idx], self.segs[tail_idx]);
        }

        self.sub_step = true;
    }

    /// Full step (tick 32).
    pub fn step(&mut self) {
        self.sub_step = false;

        if self.pause_ticks > 0 {
            self.pause_ticks -= 1;
            if self.pause_ticks == 0 {
                self.reset();
            }
            return;
        }

        let next_pos = self.find_best_move();

        self.head_idx = (self.head_idx + 1) & 63;
        self.segs[self.head_idx] = next_pos;

        if self.should_grow {
            self.len += 1;
            self.should_grow = false;
        }

        self.step_count += 1;

        if next_pos == self.apple_path {
            self.should_grow = true;
            if self.len >= RESTART_LEN {
                self.pause_ticks = PAUSE_TICKS;
            } else {
                self.place_apple();
            }
        }
    }

    /// Fill `host_leds` with the current animation frame.
    pub fn fill_leds(&self, host_leds: &mut [Color; TOTAL_LEDS]) {
        for led in host_leds.iter_mut() {
            *led = Color::BLACK;
        }

        if self.pause_ticks > 0 {
            return;
        }

        // 1. Paint body
        for i in (0..self.len as usize).rev() {
            let idx = (self.head_idx + 64 - i) & 63;
            let seg_pos = self.segs[idx];
            let (r, c) = pos_to_cell(seg_pos);
            let btn = cell_to_btn(r, c);
            let color = match i {
                0 => COLOR_HEAD,
                n if n % 2 == 1 => COLOR_BODY_LIGHT,
                _ => COLOR_BODY_DARK,
            };
            host_leds[btn * 2] = color;
            host_leds[btn * 2 + 1] = color;
        }

        // 2. Paint Apple
        let (ar, ac) = pos_to_cell(self.apple_path);
        let apple_btn = cell_to_btn(ar, ac);
        host_leds[apple_btn * 2] = COLOR_APPLE;
        host_leds[apple_btn * 2 + 1] = COLOR_APPLE;

        // 3. Half-step smoothing
        if self.sub_step {
            let (nr, nc) = pos_to_cell(self.next_head_path);
            let new_btn = cell_to_btn(nr, nc);

            match self.next_dir {
                Dir::Right | Dir::Up => {
                    host_leds[new_btn * 2] = COLOR_HEAD;
                }
                Dir::Left | Dir::Down => {
                    host_leds[new_btn * 2 + 1] = COLOR_HEAD;
                }
            }

            if self.tail_removing {
                let (tr, tc) = pos_to_cell(self.tail_path);
                let tail_btn = cell_to_btn(tr, tc);

                match self.tail_dir {
                    Dir::Right | Dir::Up => {
                        host_leds[tail_btn * 2 + 1] = Color::BLACK;
                    }
                    Dir::Left | Dir::Down => {
                        host_leds[tail_btn * 2] = Color::BLACK;
                    }
                }
            }
        }
    }
}

#[allow(clippy::identity_op)]
#[allow(clippy::erasing_op)]
pub const START_STATE: u64 = 0
    // Glider top left (5 cells)
    | (1 << (0 * 8 + 1))
    | (1 << (1 * 8 + 2))
    | (1 << (2 * 8 + 0))
    | (1 << (2 * 8 + 1))
    | (1 << (2 * 8 + 2))
    // Blinker middle right (3 cells)
    | (1 << (5 * 8 + 5))
    | (1 << (5 * 8 + 6))
    | (1 << (5 * 8 + 7))
    // Toad bottom left (6 cells)
    | (1 << (4 * 8 + 1))
    | (1 << (4 * 8 + 2))
    | (1 << (4 * 8 + 3))
    | (1 << (5 * 8 + 0))
    | (1 << (5 * 8 + 1))
    | (1 << (5 * 8 + 2));

pub fn next_life(state: u64) -> u64 {
    let rows: [u8; 8] = state.to_le_bytes();
    let mut next_rows = [0u8; 8];

    for r in 0..8usize {
        let r_up = (r + 7) & 7;
        let r_dn = (r + 1) & 7;

        for c in 0..8usize {
            let c_lf = (c + 7) & 7;
            let c_rt = (c + 1) & 7;

            let mut neighbors: u8 = 0;
            neighbors += (rows[r_up] >> c_lf) & 1;
            neighbors += (rows[r_up] >> c) & 1;
            neighbors += (rows[r_up] >> c_rt) & 1;

            neighbors += (rows[r] >> c_lf) & 1;
            neighbors += (rows[r] >> c_rt) & 1;

            neighbors += (rows[r_dn] >> c_lf) & 1;
            neighbors += (rows[r_dn] >> c) & 1;
            neighbors += (rows[r_dn] >> c_rt) & 1;

            let alive = ((rows[r] >> c) & 1) != 0;
            if (alive && (neighbors == 2 || neighbors == 3)) || (!alive && neighbors == 3) {
                next_rows[r] |= 1 << c;
            }
        }
    }

    u64::from_le_bytes(next_rows)
}

pub struct LifeSim {
    state: u64,
    history: [u64; 4],
    history_idx: usize,
    step_count: u16,
    pause_ticks: u8,
}

impl LifeSim {
    pub const fn new() -> Self {
        Self {
            state: START_STATE,
            history: [0; 4],
            history_idx: 0,
            step_count: 0,
            pause_ticks: 0,
        }
    }

    pub const fn state(&self) -> u64 {
        self.state
    }

    pub fn step(&mut self) {
        if self.pause_ticks > 0 {
            self.pause_ticks -= 1;
            if self.pause_ticks == 0 {
                self.reset();
            }
            return;
        }

        let next = next_life(self.state);

        // Check if state is extinct (0), repeating (in 4-state history), or reaches step limit (250)
        let is_extinct = next == 0;
        let is_repeating = self.history.contains(&next);
        let is_max_steps = self.step_count >= 250;

        if is_extinct || is_repeating || is_max_steps {
            // Clear board and pause for ~1 second (25 ticks @ 40ms) between runs
            self.state = 0;
            self.pause_ticks = 25;
        } else {
            self.history[self.history_idx] = self.state;
            self.history_idx = (self.history_idx + 1) & 3;
            self.state = next;
            self.step_count += 1;
        }
    }

    pub const fn reset(&mut self) {
        self.state = START_STATE;
        self.history = [0; 4];
        self.history_idx = 0;
        self.step_count = 0;
        self.pause_ticks = 0;
    }
}

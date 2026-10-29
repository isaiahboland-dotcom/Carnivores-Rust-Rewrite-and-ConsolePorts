//! Optional timings of a tick's parts, for finding slow frames on hardware
//! (the 3DS pause screen shows them). Off unless a clock is given.

/// The parts of `Hunt::tick` that are timed, in order.
pub const SECTIONS: [&str; 7] = [
    "player", "weapon", "dinos", "events", "fx", "afraid", "ship",
];

#[derive(Debug, Clone, Default)]
pub struct Prof {
    #[cfg(feature = "hunt-diagnostics")]
    pub diagnostic_state: Option<fn(u32, u32, u32, i32, u32)>,
    /// Milliseconds since some fixed point; `None` = not timing.
    pub clock: Option<fn() -> f64>,
    last: f64,
    this_tick: [f32; SECTIONS.len()],
    /// Per section: smoothed ms per tick, and the worst tick in the last
    /// [`Prof::reset_worst`] window.
    pub mean: [f32; SECTIONS.len()],
    pub worst: [f32; SECTIONS.len()],
}

impl Prof {
    /// Starts a tick.
    pub fn start(&mut self) {
        if let Some(c) = self.clock {
            self.last = c();
            self.this_tick = [0.0; SECTIONS.len()];
        }
    }

    /// Charges the time since the last mark to section `i`.
    pub fn mark(&mut self, i: usize) {
        if let Some(c) = self.clock {
            let now = c();
            if let Some(t) = self.this_tick.get_mut(i) {
                *t += (now - self.last) as f32;
            }
            self.last = now;
        }
    }

    /// Ends a tick.
    pub fn end(&mut self) {
        if self.clock.is_none() {
            return;
        }
        for i in 0..SECTIONS.len() {
            let t = self.this_tick[i];
            self.mean[i] = self.mean[i] * 0.95 + t * 0.05;
            self.worst[i] = self.worst[i].max(t);
        }
    }

    pub fn reset_worst(&mut self) {
        self.worst = [0.0; SECTIONS.len()];
    }
}

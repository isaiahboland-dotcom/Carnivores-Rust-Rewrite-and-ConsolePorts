//! Opt-in visual day cycle. Authored dawn is also used for dusk.
use crate::area::TimeOfDay;

pub const HALF_CYCLE_MS: u32 = 30 * 60 * 1000;
const QUARTER_MS: u32 = HALF_CYCLE_MS / 2;
const CYCLE_MS: u32 = HALF_CYCLE_MS * 2;

#[derive(Debug, Clone, Copy)]
pub struct DayCycle {
    elapsed: u32,
    /// Held where it is: time played doesn't move it.
    held: bool,
}

pub fn weights(time: TimeOfDay) -> [f32; 3] {
    let mut w = [0.0; 3];
    w[time as usize] = 1.0;
    w
}

impl DayCycle {
    pub fn new(start: TimeOfDay) -> Self {
        Self {
            elapsed: match start {
                TimeOfDay::Day => 0,
                TimeOfDay::Night => HALF_CYCLE_MS,
                TimeOfDay::Dusk => 3 * QUARTER_MS,
            },
            held: false,
        }
    }

    /// Midnight that never ends (hardcore survival): full night, held.
    pub fn endless_night() -> Self {
        Self {
            held: true,
            ..Self::new(TimeOfDay::Night)
        }
    }

    /// Advance only by simulation time actually played, never wall time.
    pub fn advance(&mut self, ms: u32) {
        if self.held {
            return;
        }
        self.elapsed = ((u64::from(self.elapsed) + u64::from(ms)) % u64::from(CYCLE_MS)) as u32;
    }
    /// Night runs from the dusk stop to the dawn stop (half the cycle).
    pub fn is_night(self) -> bool {
        self.elapsed >= QUARTER_MS && self.elapsed < 3 * QUARTER_MS
    }

    pub fn weights(self) -> [f32; 3] {
        let phase = (self.elapsed / QUARTER_MS) as usize;
        let t = (self.elapsed % QUARTER_MS) as f32 / QUARTER_MS as f32;
        let t = t * t * (3.0 - 2.0 * t);
        let stops = [1, 0, 2, 0, 1];
        let mut w = [0.0; 3];
        w[stops[phase]] = 1.0 - t;
        w[stops[phase + 1]] = t;
        w
    }
    pub fn sun_pos(self) -> [f32; 3] {
        let w = self.weights();
        let positions = [
            TimeOfDay::Dusk.sun_pos(),
            TimeOfDay::Day.sun_pos(),
            TimeOfDay::Night.sun_pos(),
        ];
        core::array::from_fn(|k| (0..3).map(|i| positions[i][k] * w[i]).sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn starts_at_selection_and_completes_hour() {
        for start in [TimeOfDay::Day, TimeOfDay::Dusk, TimeOfDay::Night] {
            let mut c = DayCycle::new(start);
            assert_eq!(c.weights(), weights(start));
            c.advance(CYCLE_MS);
            assert_eq!(c.weights(), weights(start));
        }
    }
    #[test]
    fn endless_night_stays_full_night() {
        let mut c = DayCycle::endless_night();
        for _ in 0..10 {
            assert!(c.is_night());
            assert_eq!(c.weights(), weights(TimeOfDay::Night));
            c.advance(QUARTER_MS);
        }
        c.advance(u32::MAX);
        assert_eq!(c.weights(), weights(TimeOfDay::Night));
    }
    #[test]
    fn day_night_and_dawn_order() {
        let mut c = DayCycle::new(TimeOfDay::Day);
        c.advance(QUARTER_MS);
        assert_eq!(c.weights(), weights(TimeOfDay::Dusk));
        c.advance(QUARTER_MS);
        assert_eq!(c.weights(), weights(TimeOfDay::Night));
        c.advance(QUARTER_MS);
        assert_eq!(c.weights(), weights(TimeOfDay::Dusk));
        c.advance(QUARTER_MS);
        assert_eq!(c.weights(), weights(TimeOfDay::Day));
    }
    #[test]
    fn smooth_bounded_and_independent_of_tick_size() {
        let mut a = DayCycle::new(TimeOfDay::Night);
        let mut b = a;
        for _ in 0..1000 {
            a.advance(10);
        }
        b.advance(10000);
        assert_eq!(a.weights(), b.weights());
        for _ in 0..3600 {
            a.advance(1000);
            let w = a.weights();
            assert!((w.iter().sum::<f32>() - 1.0).abs() < 0.00001);
            assert!(w.iter().all(|v| (0.0..=1.0).contains(v)));
        }
        a.advance(u32::MAX);
        assert!(a.weights().iter().all(|v| v.is_finite()));
    }
    #[test]
    fn area_blends_light_and_fog_without_changing_hunt_rules() {
        let mut area = crate::area::Area::flat(0);
        for (i, map) in area.map.light.iter_mut().enumerate() {
            map.fill([80, 160, 40][i]);
        }
        area.rsc.fade_rgb = [[80, 40, 20], [160, 180, 200], [20, 60, 80]];
        assert_eq!(area.light_at(0), 160.0);
        area.cycle = Some(DayCycle::new(TimeOfDay::Day));
        area.cycle.as_mut().unwrap().advance(QUARTER_MS / 2);
        assert_eq!(area.light_at(0), 120.0);
        assert_eq!(area.ground_light(512.0, 512.0), 120.0);
        assert_eq!(area.sky_rgb(), [120, 110, 110]);
        area.cycle
            .as_mut()
            .unwrap()
            .advance(HALF_CYCLE_MS - QUARTER_MS / 2);
        assert_eq!(area.light_at(0), 40.0);
        assert_eq!(area.sky_rgb(), [0, 60, 0]);
        assert_eq!(area.time, TimeOfDay::Day);
    }
    #[test]
    fn night_bonus_interval_runs_from_dusk_to_dawn() {
        let mut c = DayCycle::new(TimeOfDay::Day);
        c.advance(QUARTER_MS - 1);
        assert!(!c.is_night());
        c.advance(1);
        assert!(c.is_night());
        c.advance(HALF_CYCLE_MS - 1);
        assert!(c.is_night());
        c.advance(1);
        assert!(!c.is_night());
        assert!(!DayCycle::new(TimeOfDay::Dusk).is_night());
        assert!(DayCycle::new(TimeOfDay::Night).is_night());
    }
}

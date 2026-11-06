//! The survival's wave box (top left, under the weapon's rounds) and its
//! messages.

use super::*;

/// The wave box's colour of text.
const GOLD: [f32; 4] = [0.95, 0.85, 0.2, 1.0];

impl HuntView {
    /// Where the rounds just drawn (`ammo`) end on screen, at the lowest
    /// they reached since this weapon was drawn: firing shortens the row
    /// but the box under it stays put. 0 with none drawn yet.
    pub(super) fn ammo_bottom(&mut self, ammo: &[Sprite]) -> f32 {
        let bottom = ammo
            .iter()
            .filter_map(|s| {
                let h = s
                    .size
                    .map(|z| z[1])
                    .or_else(|| Some(self.hud.image(s.image)?.h as f32 * s.scale))?;
                Some(s.y + h)
            })
            .fold(0.0, f32::max);
        let cur = self.hunt.weapons.current;
        if self.ammo_low.0 != cur {
            self.ammo_low = (cur, 0.0);
        }
        self.ammo_low.1 = self.ammo_low.1.max(bottom);
        self.ammo_low.1
    }

    /// The wave and kills (top left, under the rounds that end at
    /// `ammo_bottom`), and the survival's messages.
    pub(super) fn draw_survival(
        &self,
        sprites: &mut Vec<Sprite>,
        ammo_bottom: f32,
        screen: (u32, u32),
        scale: f32,
    ) {
        let Some(s) = &self.hunt.survival else {
            return;
        };
        let px = 8.0 * scale;
        let wave = if s.wave == 0 {
            "Get ready".to_string()
        } else {
            format!("Wave {}", s.wave)
        };
        let lines = [wave, format!("Kills: {}", self.hunt.kills.len())];
        let (x, y) = (12.0 * scale, box_top(ammo_bottom, scale));
        let w = lines
            .iter()
            .map(|l| self.hud.label_width(px, l))
            .fold(0.0, f32::max)
            + 16.0 * scale;
        sprites.extend(self.hud.rect(x, y, w, 36.0 * scale, [0.0, 0.0, 0.0, 0.7]));
        for (i, l) in lines.iter().enumerate() {
            let ly = y + (6.0 + 12.0 * i as f32) * scale;
            sprites.extend(self.hud.label(x + 8.0 * scale, ly, px, GOLD, l));
        }
        if let Some((m, _)) = &self.message {
            let w = self.hud.label_width(px * 2.0, m);
            let (x, y) = ((screen.0 as f32 - w) / 2.0, screen.1 as f32 * 0.25);
            sprites.extend(self.hud.label(x, y, px * 2.0, GOLD, m));
        }
    }
}

/// The wave box's top: a gap under the rounds, never above where it was
/// drawn with none (12 px in the 640x480 layout).
fn box_top(ammo_bottom: f32, scale: f32) -> f32 {
    (ammo_bottom + 4.0 * scale).max(12.0 * scale)
}

#[cfg(test)]
mod tests {
    use super::box_top;

    #[test]
    fn the_box_sits_under_the_rounds() {
        assert_eq!(box_top(0.0, 1.0), 12.0);
        assert_eq!(box_top(0.0, 2.0), 24.0);
        // A row of shells 5..29 px at scale 1: the box starts at 33.
        assert_eq!(box_top(29.0, 1.0), 33.0);
    }
}

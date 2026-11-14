//! The hunt's memory plan in the menu (`crate::budget`): whether the picks
//! fit, what the top screen says about it ("Dinos up to N, ambient M"),
//! and the hunt options' secret-animals row.

use c2_formats::bits::Bits;

use super::{Menu, Pick, TIMES};
use crate::rules::Role;

/// What the plan gives the picks, and what the console has (KiB: linear,
/// then main memory, each used and in all).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Memory {
    pub linear: (usize, usize),
    pub heap: (usize, usize),
    /// Whether the hunt fits: the area and one of each pick.
    pub fits: bool,
    /// False for a pack without the plan's figures, and a survival: the
    /// counts below are not set.
    pub planned: bool,
    /// The most picked animals, secret, ambient and aquatic ones.
    pub dinos: usize,
    pub secrets: usize,
    pub ambients: usize,
    pub aquatic: usize,
    /// Memory to spare, KiB: the scarcer of linear and main.
    pub spare: usize,
    /// When it doesn't fit: the menu animal to drop first.
    pub drop: Option<usize>,
}

/// Area, time, animals, weapons, ambient on, random ambients, secret
/// animals, density.
pub(super) type MemKey = (usize, usize, Bits, Bits, bool, bool, i32, i32);

/// Hunt options: the secret animals' count (cycles).
pub(super) const O_SECRET: [f32; 4] = [40.0, 168.0, 240.0, 24.0];
/// The counts the row cycles through (the PC's slider goes to 255).
const SECRET_STEPS: [i32; 8] = [0, 1, 3, 5, 10, 20, 50, 255];

impl Menu {
    /// The picks' memory plan against what the console has (`c2-cook`'s
    /// figures; which model an animal uses can depend on the area in
    /// Modder's Edition rules).
    pub(super) fn memory(&self) -> Memory {
        let key = (
            self.area,
            self.time,
            self.dino_mask(),
            self.picked_weapons,
            self.settings.ambient,
            self.settings.random_ambients,
            self.settings.secret_dinos,
            self.save.options.dens,
        );
        if let Some((k, m)) = self.mem.get() {
            if k == key {
                return m;
            }
        }
        let i = &self.info;
        let setup = self.setup();
        let rules = self.rules.hunt_with(
            &setup.area,
            TIMES[self.time].0,
            setup.targets,
            &setup.roamers,
        );
        // "Random ambients" draws its four species when the hunt loads:
        // the first four stand in for them here.
        let ambients = setup.random_ambients.then(|| {
            (0..rules.roles.len())
                .filter(|&c| rules.roles[c] == Role::Ambient)
                .take(4)
                .collect()
        });
        let inst = crate::instance_rules::get();
        let plan = setup.plan(i, &rules, &inst, self.save.options.dens, ambients);
        // The pick to drop, as the menu lists it.
        let drop = plan.drop.and_then(|c| {
            self.dinos.iter().position(|d| match d.0 {
                Pick::Slot(k) => rules.roles.get(c) == Some(&Role::Slot(k)),
                Pick::Roamer(r) => r.char == c,
            })
        });
        let m = Memory {
            linear: (plan.used.linear, i.linear_kb),
            heap: (plan.used.heap, i.heap_kb),
            fits: plan.fits,
            planned: plan.planned,
            dinos: plan.huntables_max(),
            secrets: plan.secrets(),
            ambients: plan.ambients_max,
            aquatic: plan.aquatic.iter().map(|a| a.1).sum(),
            spare: plan.spare.linear.min(plan.spare.heap),
            drop,
        };
        self.mem.set(Some((key, m)));
        m
    }

    /// The top screen's line for a hunt that fits: the plan, or for a
    /// pack without its figures the memory the picks take.
    pub(super) fn plan_line(&self) -> String {
        let m = self.memory();
        if !m.planned || self.trophy() {
            return format!(
                "Ready. Memory {}/{} + {}/{} MiB",
                m.heap.0 / 1024,
                m.heap.1 / 1024,
                m.linear.0 / 1024,
                m.linear.1 / 1024
            );
        }
        format!(
            "Ready. Dinos up to {}, ambient {}, spare {:.1} MiB",
            m.dinos,
            m.ambients,
            m.spare as f32 / 1024.0
        )
    }

    /// The secret and aquatic animals the plan has room for, if any.
    pub(super) fn plan_extras(&self) -> Option<String> {
        let m = self.memory();
        let mut parts = Vec::new();
        if m.secrets > 0 {
            parts.push(format!("secrets {}", m.secrets));
        }
        if m.aquatic > 0 {
            parts.push(format!("aquatic {}", m.aquatic));
        }
        (m.planned && m.fits && !parts.is_empty()).then(|| format!("Also: {}", parts.join(", ")))
    }

    /// "Too much for this console", with the animal to drop when the plan
    /// names one.
    pub(super) fn too_much_line(&self) -> String {
        let name = self.memory().drop.and_then(|i| self.dinos.get(i));
        match name {
            Some(d) => {
                let n: String = d.1.name.chars().take(18).collect();
                format!("Too much for this console: drop {n}")
            }
            None => "Too much for this console".into(),
        }
    }

    /// The secret-animals row is offered for games whose own tables bring
    /// unpicked animals (Modder's Edition rules), unless the instance
    /// fixes the count.
    pub(super) fn secrets_offered(&self) -> bool {
        self.rules.dialect() && !crate::instance_rules::locked(c2_formats::rules::Lock::SecretDinos)
    }

    /// A tap on the secret-animals row: the next count, and the notice
    /// that memory can allow fewer.
    pub(super) fn tap_secrets(&mut self) {
        let now = self.settings.secret_dinos;
        let next = SECRET_STEPS
            .iter()
            .copied()
            .find(|&s| s > now)
            .unwrap_or(SECRET_STEPS[0]);
        self.settings.secret_dinos = next;
        self.focus = Some(if next == 0 {
            "No secret animals from the map's own tables".into()
        } else {
            format!("Up to {next}: fewer come when memory is short")
        });
    }

    /// The row's label.
    pub(super) fn secrets_label(&self) -> String {
        match self.settings.secret_dinos {
            0 => "Secret animals: NONE".into(),
            n => format!("Secret animals: up to {n}"),
        }
    }
}

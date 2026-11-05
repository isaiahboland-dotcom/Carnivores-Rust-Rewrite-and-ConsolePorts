//! Multi ammo's question (a setting): with it on, picking Tranq on the gear
//! page turns the Tranq / Observe buttons into **Half tranq** and **Full
//! tranq** until one is tapped. Half splits every weapon's rounds between
//! lethal ones and darts (the bottom screen's ammo switch swaps them);
//! Full is the original's tranquilizer.

use super::Menu;
use crate::draw2d::inside;

impl Menu {
    /// This hunt's tranquilizer is multi ammo's Half tranq.
    pub(super) fn half_tranq(&self) -> bool {
        self.save.options.tranq != 0 && self.settings.multi_ammo && self.settings.half_tranq
    }

    /// A tap while asking: Half (the Tranq button) or Full (Observe);
    /// elsewhere on the row keeps asking.
    pub(super) fn answer_tranq(&mut self, x: f32, y: f32) {
        let half = if inside(Self::TRANQ, x, y) {
            true
        } else if inside(Self::OBSERVE, x, y) {
            false
        } else {
            return;
        };
        self.settings.half_tranq = half;
        let _ = self.settings.store();
        self.tranq_ask = false;
        self.focus = Some(
            if half {
                "Half tranq: ammo switch in the hunt"
            } else {
                "Full tranq: darts only"
            }
            .into(),
        );
    }
}

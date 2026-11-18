//! The Advanced DHC OS UI's map previews: an area's low-resolution relief
//! (`relief.rs`: its heights every 8 cells, no objects) turning slowly in
//! the carousel's centre. A relief is built on first use, on a worker
//! thread so browsing never waits for a map to be read, and kept for the
//! pack's session, so coming back to a map shows it at once. Only the
//! highlighted map is drawn in 3D; its neighbours are pictures.
//!
//! Previews are built from the map files alone: nothing here touches a
//! hunt, its state or its random numbers.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use c2_core::model::PreparedModel;

use super::viewer::{Subject, Viewer};
use crate::gpu::Gpu;

/// Reliefs kept (each a 129x129 grid and its texture, well under 1 MB).
const KEEP: usize = 8;

/// A relief being built: the area (its column item) and the worker's answer.
struct Build {
    item: usize,
    rx: Receiver<Option<PreparedModel>>,
}

#[derive(Default)]
pub(in crate::menu) struct Previews {
    /// Built reliefs by column item, the latest used last.
    cache: Vec<(usize, PreparedModel)>,
    build: Option<Build>,
    /// Areas whose relief can't be built (their picture stands in).
    failed: Vec<usize>,
    /// The relief drawn now, and its area.
    shown: Option<(usize, Viewer)>,
    /// Build on the calling thread (a screenshot takes one frame).
    pub(in crate::menu) inline: bool,
}

impl Previews {
    /// Keeps the preview of area `want` (its column item and map file)
    /// ready: shown when built, else asked of the worker; `next` are the
    /// areas worth building ahead (the neighbours), once nothing waits.
    /// None: nothing is drawn in 3D (the carousel moves).
    pub(in crate::menu) fn update(
        &mut self,
        gpu: &Gpu,
        dt: f32,
        want: Option<(usize, PathBuf)>,
        next: &[(usize, PathBuf)],
    ) {
        self.poll();
        let Some((item, map)) = want else {
            self.shown = None;
            return;
        };
        if self.shown.as_ref().is_some_and(|(i, _)| *i != item) {
            self.shown = None;
        }
        if self.shown.is_none() {
            if !self.cache.iter().any(|(i, _)| *i == item) {
                self.ask(item, map);
            }
            if let Some(k) = self.cache.iter().position(|(i, _)| *i == item) {
                let entry = self.cache.remove(k);
                let mut viewer = Viewer::new(gpu, Subject::Terrain(entry.1.clone()));
                // The whole island in the carousel's box, corners and all.
                viewer.back_off(1.45);
                self.cache.push(entry);
                self.shown = Some((item, viewer));
            }
        }
        for (i, m) in next {
            self.ask(*i, m.clone());
        }
        if let Some((_, w)) = &mut self.shown {
            w.spin(dt);
        }
    }

    /// The relief drawn now, if it is ready.
    pub(in crate::menu) fn viewer(&mut self) -> Option<&mut Viewer> {
        self.shown.as_mut().map(|(_, w)| w)
    }

    /// Whether a relief is ready to draw.
    pub(in crate::menu) fn ready(&self) -> bool {
        self.shown.is_some()
    }

    /// Whether area `item`'s preview is still being built.
    pub(in crate::menu) fn building(&self, item: usize) -> bool {
        self.build.as_ref().is_some_and(|b| b.item == item)
    }

    /// Starts building area `item`'s relief, unless it is built, failed,
    /// or another build runs (one at a time).
    fn ask(&mut self, item: usize, map: PathBuf) {
        let known = self.cache.iter().any(|(i, _)| *i == item) || self.failed.contains(&item);
        if known || self.build.is_some() {
            return;
        }
        if self.inline {
            match crate::menu::specimens::relief(&map) {
                Some(m) => self.keep(item, m),
                None => self.failed.push(item),
            }
            return;
        }
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("dhc-preview".into())
            .spawn(move || {
                let _ = tx.send(crate::menu::specimens::relief(&map));
            });
        match spawned {
            Ok(_) => self.build = Some(Build { item, rx }),
            Err(e) => {
                tracing::warn!("map preview worker: {e}");
                self.failed.push(item);
            }
        }
    }

    /// Takes in a finished build.
    fn poll(&mut self) {
        let Some(b) = &self.build else { return };
        let got = match b.rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => None,
        };
        let item = b.item;
        self.build = None;
        match got {
            Some(model) => self.keep(item, model),
            None => self.failed.push(item),
        }
    }

    /// Keeps a built relief, dropping the least recently used past KEEP.
    fn keep(&mut self, item: usize, model: PreparedModel) {
        if self.cache.len() >= KEEP {
            self.cache.remove(0);
        }
        self.cache.push((item, model));
    }
}

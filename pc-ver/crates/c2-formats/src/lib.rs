//! Parsers and writers for Carnivores 2 data files.
//!
//! Every layout here is taken from the original source (`Resources.cpp`,
//! `Hunt.h`) and documented in `docs/FORMATS.md`. All multi-byte values are
//! little-endian. Parsers never panic on input: every failure is a
//! [`FormatError`] carrying the file offset.
//!
//! Parsers return data *as stored on disk*. Load-time transforms the original
//! applies (vertex `*2` scaling, `z` flip, texture brightening, face sorting)
//! belong to the engine, not here.

mod error;
mod reader;

pub mod car;
pub mod cooked;
pub mod map;
pub mod menu_txt;
pub mod model;
pub mod res_txt;
pub mod rsc;
pub mod sav;
pub mod tga;
pub mod three_df;
pub mod wav;

pub use error::{FormatError, FormatErrorKind};

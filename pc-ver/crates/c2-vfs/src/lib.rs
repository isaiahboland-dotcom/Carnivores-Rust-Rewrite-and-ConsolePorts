//! Layered virtual filesystem (milestone M4).
//!
//! Layers stack `base game → mod 1 → … → user overrides`; lookups are
//! case-insensitive and report which layer a file came from.

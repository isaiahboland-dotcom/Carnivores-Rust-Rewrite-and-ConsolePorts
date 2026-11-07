//! What a scenario run records: tick-indexed events and per-stage results.

#[cfg(not(feature = "std"))]
use alloc::string::String;

/// The kind of a recorded event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// A stage began.
    StageStart,
    /// A scripted action was sent (a key press, a frontend screen toggle).
    Action,
    /// A check on the hunt's state passed or failed.
    Verify,
    /// A fixed point to capture (screenshot, state hash).
    Checkpoint,
    /// A stage ended (see its [`StageResult`]).
    StageEnd,
}

impl EventKind {
    pub fn key(self) -> &'static str {
        match self {
            EventKind::StageStart => "stage_start",
            EventKind::Action => "action",
            EventKind::Verify => "verify",
            EventKind::Checkpoint => "checkpoint",
            EventKind::StageEnd => "stage_end",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub tick: u32,
    pub kind: EventKind,
    pub stage: String,
    /// Action or check name, or the checkpoint's name.
    pub name: String,
    /// `Some` for checks.
    pub ok: Option<bool>,
    pub detail: String,
}

/// A stage's outcome. Unsupported is never counted as passed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Passed,
    Failed(String),
    Unsupported(String),
}

impl Status {
    pub fn key(&self) -> &'static str {
        match self {
            Status::Passed => "passed",
            Status::Failed(_) => "failed",
            Status::Unsupported(_) => "unsupported",
        }
    }

    pub fn reason(&self) -> &str {
        match self {
            Status::Passed => "",
            Status::Failed(r) | Status::Unsupported(r) => r,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StageResult {
    pub name: String,
    pub status: Status,
    pub start_tick: u32,
    pub end_tick: u32,
}

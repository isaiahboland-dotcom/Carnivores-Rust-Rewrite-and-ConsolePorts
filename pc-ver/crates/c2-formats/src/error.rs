use std::fmt;

/// A parse failure at a specific byte offset.
#[derive(Debug, thiserror::Error)]
#[error("{context} at offset {offset:#x}: {kind}")]
pub struct FormatError {
    /// Byte offset in the input where the problem was found.
    pub offset: usize,
    /// What was being read, e.g. `"car.face[12]"`.
    pub context: String,
    pub kind: FormatErrorKind,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FormatErrorKind {
    /// Input ended `needed` bytes early.
    UnexpectedEof { needed: usize },
    /// A count or size field holds an impossible value.
    BadCount(i64),
    /// Bytes left over after a format that should consume the whole file.
    TrailingData(usize),
    /// Malformed text input.
    Syntax(String),
}

impl fmt::Display for FormatErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof { needed } => {
                write!(f, "unexpected end of data ({needed} more bytes needed)")
            }
            Self::BadCount(n) => write!(f, "invalid count/size {n}"),
            Self::TrailingData(n) => write!(f, "{n} unexpected trailing bytes"),
            Self::Syntax(s) => write!(f, "syntax error: {s}"),
        }
    }
}

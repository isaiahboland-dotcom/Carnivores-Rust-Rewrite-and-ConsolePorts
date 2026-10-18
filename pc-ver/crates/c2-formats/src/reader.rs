//! Little-endian cursor over a byte slice, plus the matching writer helpers.

use crate::error::{FormatError, FormatErrorKind};

pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn err(&self, context: impl Into<String>, kind: FormatErrorKind) -> FormatError {
        FormatError {
            offset: self.pos,
            context: context.into(),
            kind,
        }
    }

    pub fn bytes(&mut self, n: usize, ctx: &str) -> Result<&'a [u8], FormatError> {
        if self.remaining() < n {
            return Err(self.err(
                ctx,
                FormatErrorKind::UnexpectedEof {
                    needed: n - self.remaining(),
                },
            ));
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn array<const N: usize>(&mut self, ctx: &str) -> Result<[u8; N], FormatError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.bytes(N, ctx)?);
        Ok(out)
    }

    pub fn u16(&mut self, ctx: &str) -> Result<u16, FormatError> {
        Ok(u16::from_le_bytes(self.array(ctx)?))
    }

    pub fn i16(&mut self, ctx: &str) -> Result<i16, FormatError> {
        Ok(i16::from_le_bytes(self.array(ctx)?))
    }

    pub fn i32(&mut self, ctx: &str) -> Result<i32, FormatError> {
        Ok(i32::from_le_bytes(self.array(ctx)?))
    }

    pub fn f32(&mut self, ctx: &str) -> Result<f32, FormatError> {
        Ok(f32::from_le_bytes(self.array(ctx)?))
    }

    /// Reads a non-negative `i32` count, rejecting values above `max`.
    pub fn count(&mut self, max: usize, ctx: &str) -> Result<usize, FormatError> {
        let start = self.pos;
        let n = self.i32(ctx)?;
        match usize::try_from(n) {
            Ok(n) if n <= max => Ok(n),
            _ => Err(FormatError {
                offset: start,
                context: ctx.into(),
                kind: FormatErrorKind::BadCount(n.into()),
            }),
        }
    }

    /// Reads a count of records of `elem_size` bytes each, rejecting counts
    /// the remaining input could not possibly hold. This bounds allocations
    /// by the input size instead of by arbitrary engine limits.
    pub fn count_of(&mut self, elem_size: usize, ctx: &str) -> Result<usize, FormatError> {
        let max = self.remaining().saturating_sub(4) / elem_size.max(1);
        self.count(max, ctx)
    }

    /// Reads `n` little-endian `u16`s.
    pub fn u16_vec(&mut self, n: usize, ctx: &str) -> Result<Vec<u16>, FormatError> {
        let raw = self.bytes(
            n.checked_mul(2)
                .ok_or_else(|| self.err(ctx, FormatErrorKind::BadCount(n as i64)))?,
            ctx,
        )?;
        Ok(raw
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect())
    }

    /// Reads `n` little-endian `i16`s.
    pub fn i16_vec(&mut self, n: usize, ctx: &str) -> Result<Vec<i16>, FormatError> {
        let raw = self.bytes(
            n.checked_mul(2)
                .ok_or_else(|| self.err(ctx, FormatErrorKind::BadCount(n as i64)))?,
            ctx,
        )?;
        Ok(raw
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect())
    }

    pub fn finish(&self, ctx: &str) -> Result<(), FormatError> {
        match self.remaining() {
            0 => Ok(()),
            n => Err(self.err(ctx, FormatErrorKind::TrailingData(n))),
        }
    }
}

pub(crate) trait WriteLe {
    fn put_i32(&mut self, v: i32);
    fn put_f32(&mut self, v: f32);
    fn put_u16s(&mut self, v: &[u16]);
    fn put_i16s(&mut self, v: &[i16]);
    /// Writes a count field. Counts come from `Vec` lengths that were read
    /// from (or validated against) an `i32`, so saturation never triggers in
    /// practice.
    fn put_len(&mut self, n: usize) {
        self.put_i32(i32::try_from(n).unwrap_or(i32::MAX));
    }
}

impl WriteLe for Vec<u8> {
    fn put_i32(&mut self, v: i32) {
        self.extend_from_slice(&v.to_le_bytes());
    }
    fn put_f32(&mut self, v: f32) {
        self.extend_from_slice(&v.to_le_bytes());
    }
    fn put_u16s(&mut self, v: &[u16]) {
        self.reserve(v.len() * 2);
        for x in v {
            self.extend_from_slice(&x.to_le_bytes());
        }
    }
    fn put_i16s(&mut self, v: &[i16]) {
        self.reserve(v.len() * 2);
        for x in v {
            self.extend_from_slice(&x.to_le_bytes());
        }
    }
}

/// Returns the bytes of a fixed-size, NUL-padded name up to the first NUL.
pub(crate) fn c_str(raw: &[u8]) -> String {
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    String::from_utf8_lossy(&raw[..end]).into_owned()
}

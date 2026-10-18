//! Applying unified diffs (`diff -u` / `git diff` output) to text files,
//! for a recipe's own small data patches.
//!
//! Lines are compared without their `\r`, so a patch made on LF text applies
//! to the game's CRLF files; the result keeps the target's line endings.
//! Each hunk must match exactly, but may sit at a different line than its
//! header says (the nearest match wins).

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PatchError {
    #[error("patch has no hunks")]
    Empty,
    #[error("bad hunk header: {0}")]
    Header(String),
    #[error("hunk {hunk} (at line {line}) does not match the file")]
    NoMatch { hunk: usize, line: usize },
}

struct Hunk {
    /// 1-based start line in the original, from the header.
    start: usize,
    old: Vec<String>,
    new: Vec<String>,
}

fn parse(patch: &str) -> Result<Vec<Hunk>, PatchError> {
    let mut hunks = Vec::new();
    let mut cur: Option<Hunk> = None;
    for raw in patch.lines() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(h) = line.strip_prefix("@@") {
            if let Some(c) = cur.take() {
                hunks.push(c);
            }
            // "@@ -start[,len] +start[,len] @@"
            let start = h
                .trim()
                .strip_prefix('-')
                .and_then(|r| r.split([',', ' ']).next())
                .and_then(|n| n.parse::<usize>().ok())
                .ok_or_else(|| PatchError::Header(line.to_string()))?;
            cur = Some(Hunk {
                start,
                old: Vec::new(),
                new: Vec::new(),
            });
            continue;
        }
        let Some(h) = cur.as_mut() else {
            continue; // file headers (---, +++, diff, index)
        };
        if let Some(t) = line.strip_prefix(' ') {
            h.old.push(t.into());
            h.new.push(t.into());
        } else if let Some(t) = line.strip_prefix('-') {
            h.old.push(t.into());
        } else if let Some(t) = line.strip_prefix('+') {
            h.new.push(t.into());
        } else if line.is_empty() {
            // An empty context line whose leading space was trimmed.
            h.old.push(String::new());
            h.new.push(String::new());
        }
        // "\ No newline at end of file" and anything else: ignored.
    }
    if let Some(c) = cur {
        hunks.push(c);
    }
    if hunks.is_empty() {
        return Err(PatchError::Empty);
    }
    Ok(hunks)
}

/// Applies `patch` to `text`.
pub fn apply(text: &str, patch: &str) -> Result<String, PatchError> {
    let crlf = text.contains("\r\n");
    let trailing = text.ends_with('\n');
    let mut lines: Vec<String> = text
        .lines()
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
        .collect();
    // Hunks apply top to bottom; `shift` tracks how earlier hunks moved lines.
    let mut shift: isize = 0;
    let mut from = 0usize;
    for (i, h) in parse(patch)?.iter().enumerate() {
        let want = (h.start.saturating_sub(1) as isize + shift).max(0) as usize;
        let fits =
            |at: usize| at + h.old.len() <= lines.len() && lines[at..at + h.old.len()] == h.old[..];
        let at = (0..=lines.len())
            .filter(|&a| a >= from && fits(a))
            .min_by_key(|&a| a.abs_diff(want))
            .ok_or(PatchError::NoMatch {
                hunk: i + 1,
                line: h.start,
            })?;
        lines.splice(at..at + h.old.len(), h.new.iter().cloned());
        shift += h.new.len() as isize - h.old.len() as isize;
        from = at + h.new.len();
    }
    let eol = if crlf { "\r\n" } else { "\n" };
    let mut out = lines.join(eol);
    if trailing {
        out.push_str(eol);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_lf_patch_to_crlf_text_with_offsets() {
        let text = "a\r\nb\r\nc\r\nd\r\ne\r\n";
        // Header says line 1, the hunk is really at line 2.
        let patch = "--- a/x\n+++ b/x\n@@ -1,3 +1,3 @@\n b\n-c\n+C\n d\n@@ -5 +5,2 @@\n e\n+f\n";
        assert_eq!(
            apply(text, patch).as_deref(),
            Ok("a\r\nb\r\nC\r\nd\r\ne\r\nf\r\n")
        );
        assert_eq!(
            apply(text, "@@ -1 +1 @@\n-zzz\n+y\n"),
            Err(PatchError::NoMatch { hunk: 1, line: 1 })
        );
        assert_eq!(apply(text, "nothing"), Err(PatchError::Empty));
    }
}

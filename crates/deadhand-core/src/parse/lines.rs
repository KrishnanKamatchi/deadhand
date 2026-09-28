//! Byte offset → line/column mapping and code-line counting.

use crate::model::Span;

/// Line table for one source file.
pub struct Lines {
    /// Byte offset where each line starts.
    starts: Vec<u32>,
    /// `code_prefix[i]` = number of code lines among lines `0..i`.
    code_prefix: Vec<u32>,
}

impl Lines {
    /// Builds the table. `comments` are `(start, end)` byte ranges sorted by start.
    pub fn new(source: &str, comments: &[(u32, u32)]) -> Lines {
        let bytes = source.as_bytes();
        let mut starts = vec![0u32];
        let mut has_code = vec![false];
        let mut comments = comments.iter().peekable();
        let mut i = 0usize;
        while i < bytes.len() {
            if let Some(&&(cs, ce)) = comments.peek() {
                if i >= cs as usize {
                    let end = (ce as usize).clamp(i, bytes.len());
                    for (j, &b) in bytes[i..end].iter().enumerate() {
                        if b == b'\n' {
                            starts.push((i + j + 1) as u32);
                            has_code.push(false);
                        }
                    }
                    i = end;
                    comments.next();
                    continue;
                }
            }
            match bytes[i] {
                b'\n' => {
                    starts.push(i as u32 + 1);
                    has_code.push(false);
                }
                b' ' | b'\t' | b'\r' | 0x0b | 0x0c => {}
                _ => {
                    if let Some(last) = has_code.last_mut() {
                        *last = true;
                    }
                }
            }
            i += 1;
        }
        let mut code_prefix = Vec::with_capacity(has_code.len() + 1);
        code_prefix.push(0);
        let mut acc = 0;
        for c in has_code {
            acc += u32::from(c);
            code_prefix.push(acc);
        }
        Lines { starts, code_prefix }
    }

    /// Zero-based line index of a byte offset.
    fn line_of(&self, offset: u32) -> usize {
        self.starts.partition_point(|&s| s <= offset).saturating_sub(1)
    }

    /// 1-based line of a byte offset.
    pub fn line(&self, offset: u32) -> u32 {
        self.line_of(offset) as u32 + 1
    }

    /// Converts an oxc byte span to a 1-based line/column span.
    pub fn span(&self, start: u32, end: u32) -> Span {
        let (sl, el) = (self.line_of(start), self.line_of(end.max(start)));
        Span {
            start_line: sl as u32 + 1,
            start_col: start - self.starts[sl] + 1,
            end_line: el as u32 + 1,
            end_col: end.max(start) - self.starts[el] + 1,
        }
    }

    /// Code lines in the whole file.
    pub fn total_code(&self) -> u32 {
        self.code_prefix.last().copied().unwrap_or(0)
    }

    /// Code lines between two byte offsets (inclusive of both lines).
    pub fn code_between(&self, start: u32, end: u32) -> u32 {
        let (a, b) = (self.line_of(start), self.line_of(end.max(start)));
        self.code_prefix[b + 1] - self.code_prefix[a]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_code_lines_and_skips_comments() {
        let src = "// header\nconst a = 1;\n\n/* block\n still */\nconst b = 2; // tail\n";
        let comments = [(0, 9), (24, 42), (56, 63)];
        let l = Lines::new(src, &comments);
        assert_eq!(l.total_code(), 2);
        assert_eq!(l.line(10), 2);
        assert_eq!(l.span(10, 22).start_line, 2);
        assert_eq!(l.code_between(0, src.len() as u32 - 1), 2);
    }
}

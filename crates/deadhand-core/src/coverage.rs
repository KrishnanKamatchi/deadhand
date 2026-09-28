//! Minimal `lcov.info` reader: line coverage per source file.

use std::collections::HashMap;
use std::path::Path;

use crate::discover::to_slash;

/// Parses lcov text into `repo-relative path → covered fraction (0..=1)`.
pub fn parse_lcov(text: &str, root: &Path) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    let (mut file, mut found, mut hit) = (None::<String>, 0u64, 0u64);
    for line in text.lines() {
        let line = line.trim();
        if let Some(sf) = line.strip_prefix("SF:") {
            let p = Path::new(sf);
            let rel = p.strip_prefix(root).map(to_slash).unwrap_or_else(|_| sf.trim_start_matches("./").replace('\\', "/"));
            file = Some(rel);
            (found, hit) = (0, 0);
        } else if let Some(v) = line.strip_prefix("LF:") {
            found = v.parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("LH:") {
            hit = v.parse().unwrap_or(0);
        } else if line == "end_of_record" {
            if let Some(f) = file.take() {
                out.insert(f, if found == 0 { 1.0 } else { hit as f64 / found as f64 });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_records() {
        let text = "TN:\nSF:/repo/src/a.ts\nLF:10\nLH:5\nend_of_record\nSF:src/b.ts\nLF:4\nLH:4\nend_of_record\n";
        let c = parse_lcov(text, Path::new("/repo"));
        assert_eq!(c["src/a.ts"], 0.5);
        assert_eq!(c["src/b.ts"], 1.0);
    }
}

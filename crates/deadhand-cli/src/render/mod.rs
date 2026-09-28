//! Text and JSON renderers. Scores are rounded here and nowhere else.

mod text;

pub use text::{diff, explain, scan};

use anyhow::Result;
use deadhand_core::report::RepoReport;

/// Compact JSON with a trailing newline (large repos produce tens of MB; pipe to `jq` to read).
pub fn json(report: &RepoReport) -> Result<String> {
    let mut s = serde_json::to_string(report)?;
    s.push('\n');
    Ok(s)
}

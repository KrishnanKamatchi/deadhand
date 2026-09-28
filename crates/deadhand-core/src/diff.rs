//! Compare the working tree against a git revision.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::config::Config;
use crate::evidence::{Evidence, MetricKind};
use crate::git::GitSource;
use crate::report::{RepoReport, SCHEMA_VERSION};
use crate::Error;

/// Before/after comparison of two reports.
#[derive(Debug, Clone, Serialize)]
pub struct DiffReport {
    pub schema_version: u32,
    pub revision: String,
    pub before: f64,
    pub after: f64,
    pub delta: f64,
    pub metrics: Vec<MetricDelta>,
    /// Evidence present now but not at the revision.
    pub new_evidence: Vec<Evidence>,
    /// Evidence present at the revision but gone now.
    pub resolved_evidence: Vec<Evidence>,
}

/// Change in one metric's repo score.
#[derive(Debug, Clone, Serialize)]
pub struct MetricDelta {
    pub kind: MetricKind,
    pub label: String,
    pub before: Option<f64>,
    pub after: Option<f64>,
    pub delta: Option<f64>,
}

type Key<'a> = (MetricKind, &'a str, &'a str);

fn key(e: &Evidence) -> Key<'_> {
    (e.metric, e.path.as_str(), e.key.as_str())
}

/// Compares `before` (at `revision`) with `after` (working tree).
pub fn compare(before: &RepoReport, after: &RepoReport, revision: &str) -> DiffReport {
    let metrics = MetricKind::ALL
        .iter()
        .map(|&k| {
            let b = before.metric(k).and_then(|m| m.score);
            let a = after.metric(k).and_then(|m| m.score);
            MetricDelta { kind: k, label: k.label().into(), before: b, after: a, delta: b.zip(a).map(|(b, a)| a - b) }
        })
        .collect();
    let before_keys: BTreeSet<Key<'_>> = before.evidence.iter().map(key).collect();
    let after_keys: BTreeSet<Key<'_>> = after.evidence.iter().map(key).collect();
    DiffReport {
        schema_version: SCHEMA_VERSION,
        revision: revision.to_string(),
        before: before.maintainability,
        after: after.maintainability,
        delta: after.maintainability - before.maintainability,
        metrics,
        new_evidence: after.evidence.iter().filter(|e| !before_keys.contains(&key(e))).cloned().collect(),
        resolved_evidence: before.evidence.iter().filter(|e| !after_keys.contains(&key(e))).cloned().collect(),
    }
}

/// Scans `root` and the same directory at `revision` (via a temporary `git worktree`) and compares them.
pub fn diff_against(root: &Path, revision: &str, cfg: &Config, git: &dyn GitSource) -> Result<DiffReport, Error> {
    let root = crate::discover::canonical(root).map_err(|e| Error::Io(format!("{}: {e}", root.display())))?;
    let toplevel = PathBuf::from(git_out(&root, &["rev-parse", "--show-toplevel"])?);
    let prefix = git_out(&root, &["rev-parse", "--show-prefix"])?;
    git_out(&root, &["rev-parse", "--verify", "--quiet", &format!("{revision}^{{commit}}")])
        .map_err(|_| Error::Git(format!("unknown revision {revision:?}")))?;

    let worktree = Worktree::create(&toplevel, revision)?;
    let old_root = worktree.path.join(&prefix);
    let before = crate::analyze_with(&old_root, cfg, git)?;
    drop(worktree);

    let after = crate::analyze_with(&root, cfg, git)?;
    Ok(compare(&before, &after, revision))
}

/// A detached worktree removed on drop, whether the scan succeeded or not.
struct Worktree {
    repo: PathBuf,
    path: PathBuf,
}

impl Worktree {
    fn create(repo: &Path, revision: &str) -> Result<Worktree, Error> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let path = std::env::temp_dir().join(format!("deadhand-{}-{nanos}", std::process::id()));
        let path_str = path.to_string_lossy().into_owned();
        git_out(repo, &["worktree", "add", "--detach", "--quiet", &path_str, revision])?;
        Ok(Worktree { repo: repo.to_path_buf(), path })
    }
}

impl Drop for Worktree {
    fn drop(&mut self) {
        let path = self.path.to_string_lossy().into_owned();
        if git_out(&self.repo, &["worktree", "remove", "--force", &path]).is_err() {
            let _ = std::fs::remove_dir_all(&self.path);
            let _ = git_out(&self.repo, &["worktree", "prune"]);
        }
    }
}

fn git_out(dir: &Path, args: &[&str]) -> Result<String, Error> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| Error::Git(format!("cannot run git: {e}")))?;
    if !out.status.success() {
        return Err(Error::Git(String::from_utf8_lossy(&out.stderr).trim().to_string()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

//! Churn and ownership facts from one `git log --numstat` call.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::process::Command;

use crate::Error;

/// Source of raw git log text. A trait so the CLI shell-out can be swapped for `gix`.
pub trait GitSource {
    /// Returns the log for `root` in the format parsed by [`parse_log`], or `None` if `root` is not in a git repo.
    fn log(&self, root: &Path) -> Result<Option<String>, Error>;
}

/// Shells out to the `git` binary.
pub struct GitCli;

/// Record separator between commits, field separator inside the header.
const RS: char = '\x1e';
const FS: char = '\x1f';

impl GitSource for GitCli {
    fn log(&self, root: &Path) -> Result<Option<String>, Error> {
        let probe = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["rev-parse", "--is-inside-work-tree", "--is-shallow-repository"])
            .output();
        match probe {
            // In a shallow clone every file looks like it has a single commit.
            Ok(o) if o.status.success() && !String::from_utf8_lossy(&o.stdout).contains("\ntrue") => {}
            _ => return Ok(None), // no git binary, not a repository, or shallow
        }
        let out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["-c", "core.quotePath=false", "log", "--no-merges", "--no-renames", "--numstat", "--relative"])
            .arg("--format=%x1e%H%x1f%ae%x1f%ct")
            .output()
            .map_err(|e| Error::Git(e.to_string()))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            if err.contains("does not have any commits") {
                return Ok(None);
            }
            return Err(Error::Git(err.trim().to_string()));
        }
        Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned()))
    }
}

/// Ignores git entirely (`--no-git`, and tests that must not depend on the outer repo).
pub struct NoGit;

impl GitSource for NoGit {
    fn log(&self, _root: &Path) -> Result<Option<String>, Error> {
        Ok(None)
    }
}

/// History of one file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileHistory {
    pub commits: u32,
    /// Commits inside the churn window.
    pub recent_commits: u32,
    pub authors: u32,
    /// Unix seconds.
    pub first_commit: i64,
    pub last_commit: i64,
    /// Total lines added by the commit that introduced the file.
    pub intro_commit_lines: u64,
}

/// Git facts for the whole repo.
#[derive(Debug, Clone, Default)]
pub struct GitFacts {
    /// Time of the newest commit; the churn window ends here, so results do not depend on the clock.
    pub head_time: i64,
    pub files: HashMap<String, FileHistory>,
}

/// Collects git facts for `root`. `Ok(None)` when git history is unavailable.
pub fn collect(root: &Path, source: &dyn GitSource, churn_days: u64) -> Result<Option<GitFacts>, Error> {
    Ok(source.log(root)?.map(|log| parse_log(&log, churn_days)))
}

struct Commit<'a> {
    author: &'a str,
    time: i64,
    added_total: u64,
    files: Vec<&'a str>,
}

/// Parses `--format=%x1e%H%x1f%ae%x1f%ct --numstat` output (newest commit first).
pub fn parse_log(log: &str, churn_days: u64) -> GitFacts {
    let commits: Vec<Commit<'_>> = log
        .split(RS)
        .filter_map(|rec| {
            let mut lines = rec.lines();
            let mut header = lines.next()?.split(FS);
            let _hash = header.next()?;
            let author = header.next()?;
            let time = header.next()?.trim().parse().ok()?;
            let mut c = Commit { author, time, added_total: 0, files: Vec::new() };
            for line in lines {
                let mut parts = line.splitn(3, '\t');
                let (Some(added), Some(_), Some(path)) = (parts.next(), parts.next(), parts.next()) else { continue };
                c.added_total += added.parse::<u64>().unwrap_or(0); // "-" for binary files
                c.files.push(path);
            }
            Some(c)
        })
        .collect();

    let head_time = commits.iter().map(|c| c.time).max().unwrap_or(0);
    let window_start = head_time - (churn_days as i64) * 86_400;

    let mut files: HashMap<String, FileHistory> = HashMap::new();
    let mut authors: HashMap<&str, BTreeSet<&str>> = HashMap::new();
    for c in &commits {
        for &path in &c.files {
            let h = files
                .entry(path.to_string())
                .or_insert_with(|| FileHistory { first_commit: i64::MAX, ..Default::default() });
            h.commits += 1;
            if c.time >= window_start {
                h.recent_commits += 1;
            }
            h.last_commit = h.last_commit.max(c.time);
            if c.time <= h.first_commit {
                h.first_commit = c.time;
                h.intro_commit_lines = c.added_total;
            }
            authors.entry(path).or_default().insert(c.author);
        }
    }
    for (path, set) in authors {
        if let Some(h) = files.get_mut(path) {
            h.authors = set.len() as u32;
        }
    }
    GitFacts { head_time, files }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numstat_log() {
        let day = 86_400;
        let log = format!(
            "\x1eh3\x1fbob@x\x1f{t3}\n\n2\t1\tsrc/a.ts\n\x1eh2\x1falice@x\x1f{t2}\n\n900\t0\tsrc/b.ts\n5\t0\tsrc/a.ts\n\x1eh1\x1falice@x\x1f{t1}\n\n10\t0\tsrc/a.ts\n-\t-\tlogo.png\n",
            t3 = 1000 * day,
            t2 = 900 * day,
            t1 = 700 * day
        );
        let g = parse_log(&log, 180);
        let a = &g.files["src/a.ts"];
        assert_eq!((a.commits, a.recent_commits, a.authors, a.intro_commit_lines), (3, 2, 2, 10));
        let b = &g.files["src/b.ts"];
        assert_eq!((b.commits, b.authors, b.intro_commit_lines), (1, 1, 905));
        assert_eq!(g.head_time, 1000 * day);
    }
}

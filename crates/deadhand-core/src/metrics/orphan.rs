//! Orphaned Code: code no person has demonstrably come back to.
//!
//! A heuristic about engagement, never about authorship. Flags per module:
//! - untouched: a single commit (one author, never edited since it was introduced)
//! - bulk: introduced by a commit adding more than `bulk_commit_lines` lines and never edited
//!   since (a later edit is evidence someone engaged with it)
//! - critical-untouched: no commits in the churn window while ≥ `critical_fan_in` modules depend on it
//!
//! Raw value = number of flags. Absolute score = 100 − 35 per flag.

use super::{values, Inputs, MetricOutput, ModuleValue};
use crate::evidence::{Evidence, MetricKind, Severity};

pub fn run(inp: &Inputs<'_>) -> MetricOutput {
    let kind = MetricKind::OrphanedCode;
    let n = inp.modules.len();
    let Some(git) = inp.git else {
        return MetricOutput::unavailable(kind, n, "no git history; Orphaned Code skipped");
    };
    let t = &inp.cfg.thresholds;
    let mut out = MetricOutput::new(kind, n);
    let (mut untouched_n, mut bulk_n, mut critical_n, mut untracked) = (0, 0, 0, 0);

    for (i, m) in inp.modules.iter().enumerate() {
        let Some(h) = git.files.get(&m.path) else {
            if !m.is_test {
                untracked += 1;
            }
            out.modules[i] = ModuleValue { raw: 0.0, abs: 100.0, values: values([("tracked", 0.0)]) };
            continue;
        };
        let fan_in = inp.graph.inc[i].len();
        let untouched = h.commits == 1;
        let bulk = untouched && h.intro_commit_lines > t.bulk_commit_lines;
        let critical = h.recent_commits == 0 && fan_in >= t.critical_fan_in;

        let mut reasons = Vec::new();
        if untouched {
            reasons.push("never edited after the commit that added it".to_string());
        }
        if bulk {
            reasons.push(format!("added in a commit of {} lines", h.intro_commit_lines));
        }
        if critical {
            reasons.push(format!("no commits in {} days while {fan_in} modules depend on it", t.churn_days));
        }
        let flags = reasons.len();
        if flags > 0 {
            let key: Vec<&str> = [(untouched, "untouched"), (bulk, "bulk"), (critical, "critical-untouched")]
                .iter()
                .filter(|(on, _)| *on)
                .map(|(_, k)| *k)
                .collect();
            out.evidence.push(Evidence {
                metric: kind,
                severity: if flags >= 2 { Severity::Warn } else { Severity::Info },
                path: m.path.clone(),
                span: None,
                key: key.join("+"),
                message: format!("{} ({} commits, {} authors)", capitalize(&reasons.join("; ")), h.commits, h.authors),
                values: vec![
                    ("commits".into(), h.commits as f64),
                    ("authors".into(), h.authors as f64),
                    ("intro_commit_lines".into(), h.intro_commit_lines as f64),
                    ("fan_in".into(), fan_in as f64),
                ],
            });
        }
        if !m.is_test {
            untouched_n += usize::from(untouched);
            bulk_n += usize::from(bulk);
            critical_n += usize::from(critical);
        }
        out.modules[i] = ModuleValue {
            raw: flags as f64,
            abs: (100.0 - 35.0 * flags as f64).max(0.0),
            values: values([
                ("commits", h.commits as f64),
                ("recent_commits", h.recent_commits as f64),
                ("authors", h.authors as f64),
                ("intro_commit_lines", h.intro_commit_lines as f64),
                ("flags", flags as f64),
            ]),
        };
    }
    if untracked > 0 {
        out.notes.push(format!("{untracked} modules have no git history yet (uncommitted)"));
    }
    out.finding(Severity::Info, untouched_n, format!("{untouched_n} modules were never edited after being added"));
    out.finding(Severity::Warn, bulk_n, format!("{bulk_n} modules were added in bulk commits (>{} lines)", t.bulk_commit_lines));
    out.finding(Severity::Warn, critical_n, format!("{critical_n} widely used modules had no commits in {} days", t.churn_days));
    out
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

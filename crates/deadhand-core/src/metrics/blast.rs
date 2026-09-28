//! Blast Radius: how much could break if this module changes?
//!
//! Raw value: transitive dependents × churn factor × (1 − coverage).
//! - churn factor = 0.5 + 1.5 × min(commits in window, 20) / 20, or 1 without git
//! - coverage is used only when an lcov file is configured; otherwise the factor is 1

use std::collections::BTreeSet;

use super::{fmt_num, linear, values, Inputs, MetricOutput, ModuleValue};
use crate::evidence::{Evidence, MetricKind, Severity};

/// Dependents at which a module counts as widely depended on.
const WIDE: usize = 20;

pub fn run(inp: &Inputs<'_>) -> MetricOutput {
    let kind = MetricKind::BlastRadius;
    let t = &inp.cfg.thresholds;
    let mut out = MetricOutput::new(kind, inp.modules.len());
    if inp.git.is_none() {
        out.notes.push("churn unknown (no git history); churn factor fixed at 1".into());
    }
    if inp.coverage.is_none() {
        out.notes.push("coverage unknown; coverage factor fixed at 1".into());
    }

    let mut wide = 0usize;
    for (i, m) in inp.modules.iter().enumerate() {
        let dependents = &inp.dependents[i];
        let direct = inp.graph.inc[i].len();
        let layers: BTreeSet<&str> = dependents.iter().filter_map(|&(j, _)| inp.layer(j)).collect();
        let churn = inp.git.map(|g| g.files.get(&m.path).map_or(0, |h| h.recent_commits));
        let churn_factor = churn.map_or(1.0, |c| 0.5 + 1.5 * (c.min(20) as f64) / 20.0);
        let coverage = inp.coverage.map(|c| c.get(&m.path).copied().unwrap_or(0.0));
        let uncovered = 1.0 - coverage.unwrap_or(0.0);
        let raw = dependents.len() as f64 * churn_factor * uncovered;

        if raw > t.blast_good {
            let churn_text =
                churn.map_or("churn unknown".to_string(), |c| format!("{c} commits in {} days", t.churn_days));
            let cov_text =
                coverage.map_or("coverage unknown".to_string(), |c| format!("{}% line coverage", fmt_num(c * 100.0)));
            out.evidence.push(Evidence {
                metric: kind,
                severity: if raw > t.blast_bad { Severity::High } else { Severity::Warn },
                path: m.path.clone(),
                span: None,
                key: "dependents".into(),
                message: format!(
                    "{} dependents ({} direct) across {} layers; {}; {}",
                    dependents.len(),
                    direct,
                    layers.len(),
                    churn_text,
                    cov_text
                ),
                values: vec![
                    ("dependents".into(), dependents.len() as f64),
                    ("direct".into(), direct as f64),
                    ("churn".into(), churn.map_or(-1.0, f64::from)),
                    ("blast".into(), raw),
                ],
            });
        }
        if dependents.len() >= WIDE && !m.is_test {
            wide += 1;
        }

        out.modules[i] = ModuleValue {
            raw,
            abs: linear(raw, t.blast_good, t.blast_bad),
            values: values([
                ("dependents", dependents.len() as f64),
                ("direct_dependents", direct as f64),
                ("layers_affected", layers.len() as f64),
                ("churn_factor", churn_factor),
                ("uncovered", uncovered),
                ("blast", raw),
            ]),
        };
    }
    out.finding(Severity::Warn, wide, format!("{wide} modules are depended on by {WIDE}+ modules"));
    out
}

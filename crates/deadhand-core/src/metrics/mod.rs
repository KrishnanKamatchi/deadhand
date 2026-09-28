//! The seven metrics. Each is a pure function: facts in, values + evidence out. No I/O.

mod blast;
mod cognitive;
mod context;
mod drift;
mod entanglement;
mod orphan;
mod readability;

use std::collections::{BTreeMap, HashMap};

use crate::config::Config;
use crate::evidence::{Evidence, Finding, MetricKind};
use crate::git::GitFacts;
use crate::graph::ImportGraph;
use crate::model::ModuleFacts;

/// Everything the metrics may read.
pub struct Inputs<'a> {
    pub modules: &'a [ModuleFacts],
    pub graph: &'a ImportGraph,
    /// Layer per module (`None` = unknown).
    pub layers: &'a [Option<String>],
    pub git: Option<&'a GitFacts>,
    /// Line coverage per module path, 0..=1, when an lcov file is configured.
    pub coverage: Option<&'a HashMap<String, f64>>,
    pub cfg: &'a Config,
    /// Forward closure (value edges) per module: `(module, distance)`.
    pub dependencies: &'a [Vec<(usize, u32)>],
    /// Reverse closure (all edges) per module.
    pub dependents: &'a [Vec<(usize, u32)>],
}

impl Inputs<'_> {
    /// Indices of non-test modules, the population for repo baselines.
    fn population(&self) -> impl Iterator<Item = usize> + '_ {
        self.modules.iter().enumerate().filter(|(_, m)| !m.is_test).map(|(i, _)| i)
    }

    fn layer(&self, i: usize) -> Option<&str> {
        self.layers[i].as_deref()
    }
}

/// A metric's result for one module.
#[derive(Debug, Clone, Default)]
pub struct ModuleValue {
    /// Primary raw value; higher is worse. Used for repo-relative ranking.
    pub raw: f64,
    /// Absolute score 0..=100 from fixed thresholds.
    pub abs: f64,
    /// Named raw values behind the score.
    pub values: BTreeMap<String, f64>,
}

/// A metric's result for the whole repo.
#[derive(Debug, Clone)]
pub struct MetricOutput {
    pub kind: MetricKind,
    pub available: bool,
    pub notes: Vec<String>,
    /// One entry per module (same order as the module list).
    pub modules: Vec<ModuleValue>,
    pub evidence: Vec<Evidence>,
    pub findings: Vec<Finding>,
}

impl MetricOutput {
    fn new(kind: MetricKind, n: usize) -> MetricOutput {
        MetricOutput { kind, available: true, notes: Vec::new(), modules: vec![ModuleValue::default(); n], evidence: Vec::new(), findings: Vec::new() }
    }

    fn unavailable(kind: MetricKind, n: usize, note: &str) -> MetricOutput {
        MetricOutput { available: false, notes: vec![note.to_string()], ..MetricOutput::new(kind, n) }
    }

    fn finding(&mut self, severity: crate::evidence::Severity, count: usize, message: String) {
        if count > 0 {
            self.findings.push(Finding { metric: self.kind, severity, count, message });
        }
    }
}

/// Runs every metric, in [`MetricKind::ALL`] order.
pub fn run_all(inp: &Inputs<'_>) -> Vec<MetricOutput> {
    MetricKind::ALL
        .iter()
        .map(|k| match k {
            MetricKind::CognitiveLoad => cognitive::run(inp),
            MetricKind::Readability => readability::run(inp),
            MetricKind::Entanglement => entanglement::run(inp),
            MetricKind::ContextDepth => context::run(inp),
            MetricKind::BlastRadius => blast::run(inp),
            MetricKind::PatternDrift => drift::run(inp),
            MetricKind::OrphanedCode => orphan::run(inp),
        })
        .collect()
}

/// 100 at or below `good`, 0 at or above `bad`, linear in between.
pub(crate) fn linear(raw: f64, good: f64, bad: f64) -> f64 {
    if bad <= good {
        return if raw <= good { 100.0 } else { 0.0 };
    }
    (100.0 * (bad - raw) / (bad - good)).clamp(0.0, 100.0)
}

/// Median of a slice (0 for empty). Sorts a copy.
pub(crate) fn median(values: &[f64]) -> f64 {
    percentile(values, 0.5)
}

/// Linear-interpolated percentile, `p` in 0..=1.
pub(crate) fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let pos = p.clamp(0.0, 1.0) * (v.len() - 1) as f64;
    let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
    v[lo] + (v[hi] - v[lo]) * (pos - lo as f64)
}

/// Median absolute deviation.
pub(crate) fn mad(values: &[f64], med: f64) -> f64 {
    let dev: Vec<f64> = values.iter().map(|v| (v - med).abs()).collect();
    median(&dev)
}

fn values<const N: usize>(pairs: [(&str, f64); N]) -> BTreeMap<String, f64> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

fn fmt_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(linear(5.0, 5.0, 20.0), 100.0);
        assert_eq!(linear(12.5, 5.0, 20.0), 50.0);
        assert_eq!(linear(30.0, 5.0, 20.0), 0.0);
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0, 5.0], 0.9), 4.6);
        assert_eq!(mad(&[1.0, 1.0, 2.0, 2.0, 4.0, 6.0, 9.0], 2.0), 1.0);
    }
}

//! The versioned, serializable result of a scan.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::evidence::{Evidence, Finding, MetricKind};
use crate::model::{FunctionFacts, Import};

/// Bumped on any breaking change to the JSON shape.
pub const SCHEMA_VERSION: u32 = 1;

/// Full scan result. Deterministic: same input and config give byte-identical JSON.
#[derive(Debug, Clone, Serialize)]
pub struct RepoReport {
    pub schema_version: u32,
    pub tool_version: String,
    pub root_name: String,
    pub summary: Summary,
    pub maintainability: f64,
    pub metrics: Vec<MetricSummary>,
    pub findings: Vec<Finding>,
    pub notes: Vec<String>,
    pub cycles: Vec<Vec<String>>,
    pub parse_errors: Vec<ParseError>,
    pub modules: Vec<ModuleReport>,
    pub evidence: Vec<Evidence>,
}

/// Counts for the scanned repo.
#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub files: usize,
    pub test_files: usize,
    pub loc: u64,
    pub functions: usize,
    pub generated_skipped: usize,
    pub git_available: bool,
}

/// Repo-level result of one metric.
#[derive(Debug, Clone, Serialize)]
pub struct MetricSummary {
    pub kind: MetricKind,
    pub label: String,
    pub available: bool,
    /// `None` when unavailable.
    pub score: Option<f64>,
    /// Weight used in Maintainability (after redistribution).
    pub weight: f64,
    pub notes: Vec<String>,
}

/// A file that failed to read or parse cleanly.
#[derive(Debug, Clone, Serialize)]
pub struct ParseError {
    pub path: String,
    pub message: String,
}

/// Everything known about one module.
#[derive(Debug, Clone, Serialize)]
pub struct ModuleReport {
    pub path: String,
    pub layer: Option<String>,
    pub is_test: bool,
    pub loc: u32,
    pub maintainability: f64,
    pub scores: BTreeMap<MetricKind, f64>,
    pub raw: BTreeMap<MetricKind, BTreeMap<String, f64>>,
    pub functions: Vec<FunctionFacts>,
    pub imports: Vec<Import>,
}

impl RepoReport {
    /// Score of one metric, if available.
    pub fn metric(&self, kind: MetricKind) -> Option<&MetricSummary> {
        self.metrics.iter().find(|m| m.kind == kind)
    }

    /// The `n` lowest-scoring non-test modules, worst first (ties broken by path).
    pub fn worst(&self, n: usize) -> Vec<&ModuleReport> {
        let mut v: Vec<&ModuleReport> = self.modules.iter().filter(|m| !m.is_test).collect();
        v.sort_by(|a, b| a.maintainability.total_cmp(&b.maintainability).then_with(|| a.path.cmp(&b.path)));
        v.truncate(n);
        v
    }

    /// Finds a module by repo-relative path.
    pub fn module(&self, path: &str) -> Option<&ModuleReport> {
        self.modules.iter().find(|m| m.path == path)
    }

    /// Evidence for one module.
    pub fn evidence_for<'a>(&'a self, path: &'a str) -> impl Iterator<Item = &'a Evidence> + 'a {
        self.evidence.iter().filter(move |e| e.path == path)
    }
}

impl ModuleReport {
    /// Metrics that pull this module down the most (score < 75), weakest first, at most `n`.
    pub fn weakest(&self, n: usize) -> Vec<MetricKind> {
        let mut v: Vec<(MetricKind, f64)> = self.scores.iter().filter(|(_, s)| **s < 75.0).map(|(k, s)| (*k, *s)).collect();
        v.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        v.into_iter().take(n).map(|(k, _)| k).collect()
    }
}

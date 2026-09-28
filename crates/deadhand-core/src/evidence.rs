//! Evidence: the concrete, observable facts behind every score.

use serde::Serialize;

use crate::config::Weights;
use crate::model::Span;

/// The seven metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricKind {
    CognitiveLoad,
    Readability,
    Entanglement,
    ContextDepth,
    BlastRadius,
    PatternDrift,
    OrphanedCode,
}

impl MetricKind {
    /// All metrics in report order.
    pub const ALL: [MetricKind; 7] = [
        MetricKind::Readability,
        MetricKind::CognitiveLoad,
        MetricKind::Entanglement,
        MetricKind::ContextDepth,
        MetricKind::BlastRadius,
        MetricKind::PatternDrift,
        MetricKind::OrphanedCode,
    ];

    /// Human-readable name.
    pub fn label(self) -> &'static str {
        match self {
            MetricKind::CognitiveLoad => "Cognitive Load",
            MetricKind::Readability => "Readability",
            MetricKind::Entanglement => "Entanglement",
            MetricKind::ContextDepth => "Context Depth",
            MetricKind::BlastRadius => "Blast Radius",
            MetricKind::PatternDrift => "Pattern Drift",
            MetricKind::OrphanedCode => "Orphaned Code",
        }
    }

    /// Configured weight in Maintainability.
    pub fn weight(self, w: &Weights) -> f64 {
        match self {
            MetricKind::CognitiveLoad => w.cognitive_load,
            MetricKind::Readability => w.readability,
            MetricKind::Entanglement => w.entanglement,
            MetricKind::ContextDepth => w.context_depth,
            MetricKind::BlastRadius => w.blast_radius,
            MetricKind::PatternDrift => w.pattern_drift,
            MetricKind::OrphanedCode => w.orphaned_code,
        }
    }
}

/// How serious a piece of evidence is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warn,
    High,
}

/// One observed fact tied to a file (and optionally a location).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Evidence {
    pub metric: MetricKind,
    pub severity: Severity,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    /// Stable identity used by `diff` to match findings across revisions (no numbers in it).
    pub key: String,
    pub message: String,
    pub values: Vec<(String, f64)>,
}

/// A repo-level headline, e.g. "6 modules participate in circular dependencies".
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    pub metric: MetricKind,
    pub severity: Severity,
    pub count: usize,
    pub message: String,
}

/// Sorts evidence by path, then span, then metric and key.
pub fn sort_evidence(ev: &mut [Evidence]) {
    ev.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| a.span.cmp(&b.span))
            .then_with(|| a.metric.cmp(&b.metric))
            .then_with(|| a.key.cmp(&b.key))
    });
}

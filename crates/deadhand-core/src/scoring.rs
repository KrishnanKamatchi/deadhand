//! Raw metric values → 0..100 scores (100 = easy for humans).
//!
//! Module score = `absolute_share` × absolute + (1 − `absolute_share`) × relative, where
//! relative = 100 × (1 − better / (n − 1)) over non-test modules, and a module with a perfect
//! absolute score also gets a perfect relative one (so a uniformly clean repo is not penalised).
//! Repo score per metric = LOC-weighted mean of non-test module scores.
//! Maintainability = weighted mean of available metrics; weights of unavailable ones are
//! redistributed proportionally.

use crate::config::Config;
use crate::evidence::MetricKind;
use crate::metrics::MetricOutput;
use crate::model::ModuleFacts;

/// Scores for one metric.
#[derive(Debug, Clone)]
pub struct MetricScores {
    pub kind: MetricKind,
    pub available: bool,
    /// Per module, same order as modules.
    pub modules: Vec<f64>,
    /// Repo-level score; `None` when unavailable or no non-test modules.
    pub repo: Option<f64>,
    /// Weight actually used in Maintainability after redistribution.
    pub effective_weight: f64,
}

/// All scores.
#[derive(Debug, Clone)]
pub struct Scores {
    pub metrics: Vec<MetricScores>,
    pub module_maintainability: Vec<f64>,
    pub maintainability: f64,
    /// Set when weights were redistributed.
    pub redistribution_note: Option<String>,
}

/// Scores every metric output.
pub fn score(outputs: &[MetricOutput], modules: &[ModuleFacts], cfg: &Config) -> Scores {
    let share = cfg.scoring.absolute_share;
    let pop: Vec<usize> = modules.iter().enumerate().filter(|(_, m)| !m.is_test).map(|(i, _)| i).collect();

    let total_weight: f64 = outputs.iter().map(|o| o.kind.weight(&cfg.weights)).sum();
    let available_weight: f64 = outputs.iter().filter(|o| o.available).map(|o| o.kind.weight(&cfg.weights)).sum();

    let mut metrics = Vec::with_capacity(outputs.len());
    for o in outputs {
        let effective_weight = if o.available && available_weight > 0.0 { o.kind.weight(&cfg.weights) / available_weight } else { 0.0 };
        if !o.available {
            metrics.push(MetricScores { kind: o.kind, available: false, modules: vec![100.0; modules.len()], repo: None, effective_weight });
            continue;
        }
        let mut pop_raw: Vec<f64> = pop.iter().map(|&i| o.modules[i].raw).collect();
        pop_raw.sort_by(f64::total_cmp);
        let n = pop_raw.len();
        let module_scores: Vec<f64> = o
            .modules
            .iter()
            .map(|v| {
                let rel = if v.abs >= 100.0 || n <= 1 {
                    100.0
                } else {
                    let better = pop_raw.partition_point(|&r| r < v.raw).min(n - 1);
                    100.0 * (1.0 - better as f64 / (n - 1) as f64)
                };
                share * v.abs + (1.0 - share) * rel
            })
            .collect();
        let repo = loc_weighted(&pop, modules, &module_scores);
        metrics.push(MetricScores { kind: o.kind, available: true, modules: module_scores, repo, effective_weight });
    }

    let module_maintainability: Vec<f64> =
        (0..modules.len()).map(|i| metrics.iter().map(|m| m.effective_weight * m.modules[i]).sum()).collect();
    let maintainability = metrics.iter().filter_map(|m| m.repo.map(|r| r * m.effective_weight)).sum::<f64>();
    let maintainability = if pop.is_empty() { 100.0 } else { maintainability };

    let missing: Vec<&str> = outputs.iter().filter(|o| !o.available).map(|o| o.kind.label()).collect();
    let redistribution_note = (!missing.is_empty() && total_weight > 0.0)
        .then(|| format!("{} unavailable; its weight was redistributed across the other metrics", missing.join(", ")));

    Scores { metrics, module_maintainability, maintainability, redistribution_note }
}

fn loc_weighted(pop: &[usize], modules: &[ModuleFacts], scores: &[f64]) -> Option<f64> {
    let (mut sum, mut w) = (0.0, 0.0);
    for &i in pop {
        let weight = f64::from(modules[i].loc.max(1));
        sum += weight * scores[i];
        w += weight;
    }
    (w > 0.0).then(|| sum / w)
}

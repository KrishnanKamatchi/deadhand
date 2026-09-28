//! `deadhand.toml` loading and every default threshold and weight.
//!
//! Every key is optional; anything missing falls back to the defaults below.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Error;

/// Top-level configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Globs (repo-relative) a file must match to be scanned. Empty = everything.
    pub include: Vec<String>,
    /// Globs (repo-relative) to skip, on top of the built-in excludes.
    pub exclude: Vec<String>,
    pub layers: LayerConfig,
    pub weights: Weights,
    pub thresholds: Thresholds,
    pub scoring: ScoringConfig,
    pub coverage: CoverageConfig,
}

/// Architectural layer settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayerConfig {
    /// Allowed dependency direction, left → right. A layer may import layers to its right.
    pub order: Vec<String>,
    /// Layer name → path segments. Entries given in the file replace the default for that layer.
    pub paths: BTreeMap<String, Vec<String>>,
}

impl Default for LayerConfig {
    fn default() -> Self {
        let paths = [
            ("ui", &["components", "pages", "app", "views", "screens"][..]),
            ("route", &["routes", "controllers", "controller", "handlers", "api"]),
            ("service", &["services", "service", "usecases", "domain"]),
            ("data", &["repositories", "repository", "repo", "models", "model", "db", "prisma", "entities", "entity"]),
            ("infra", &["lib", "clients", "adapters", "integrations", "config"]),
            ("shared", &["utils", "helpers", "common", "shared"]),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
        .collect();
        Self {
            order: ["ui", "route", "service", "data", "infra"].map(String::from).to_vec(),
            paths,
        }
    }
}

/// Metric weights for the Maintainability score. Redistributed if a metric is unavailable.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Weights {
    pub cognitive_load: f64,
    pub readability: f64,
    pub entanglement: f64,
    pub context_depth: f64,
    pub blast_radius: f64,
    pub pattern_drift: f64,
    pub orphaned_code: f64,
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            cognitive_load: 0.20,
            readability: 0.15,
            entanglement: 0.20,
            context_depth: 0.15,
            blast_radius: 0.15,
            pattern_drift: 0.10,
            orphaned_code: 0.05,
        }
    }
}

/// Absolute thresholds. "good" = full marks, "bad" = zero for the absolute part of a score.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Thresholds {
    /// Function cognitive complexity above this is flagged.
    pub cognitive_complexity: u32,
    /// Cognitive complexity per 100 LOC: good / bad.
    pub cognitive_density_good: f64,
    pub cognitive_density_bad: f64,
    /// Nesting depth at or above this is flagged.
    pub max_nesting: u32,
    /// Parameter count at or above this is flagged.
    pub max_params: u32,
    /// Readability penalty points: good / bad.
    pub readability_good: f64,
    pub readability_bad: f64,
    /// Weighted internal fan-out: good / bad.
    pub fan_out_good: f64,
    pub fan_out_bad: f64,
    /// Weighted dependency closure (Σ 1/distance): good / bad.
    pub context_good: f64,
    pub context_bad: f64,
    /// Longest import chain below this module: good / bad.
    pub context_depth_good: f64,
    pub context_depth_bad: f64,
    /// Modules whose closure spans at least this many layers are flagged.
    pub context_layers: usize,
    /// Blast value (dependents × churn × uncovered): good / bad.
    pub blast_good: f64,
    pub blast_bad: f64,
    /// Drift (Σ|z| above 2 per feature, plus 1 per layer violation): good / bad.
    pub drift_good: f64,
    pub drift_bad: f64,
    /// Lines added in one commit that make it a "bulk" commit.
    pub bulk_commit_lines: u64,
    /// Churn window in days.
    pub churn_days: u64,
    /// Fan-in at which an untouched module counts as critical.
    pub critical_fan_in: usize,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            cognitive_complexity: 15,
            cognitive_density_good: 5.0,
            cognitive_density_bad: 30.0,
            max_nesting: 4,
            max_params: 5,
            readability_good: 0.0,
            readability_bad: 8.0,
            fan_out_good: 5.0,
            fan_out_bad: 20.0,
            context_good: 5.0,
            context_bad: 40.0,
            context_depth_good: 4.0,
            context_depth_bad: 12.0,
            context_layers: 4,
            blast_good: 5.0,
            blast_bad: 60.0,
            drift_good: 0.5,
            drift_bad: 3.0,
            bulk_commit_lines: 800,
            churn_days: 180,
            critical_fan_in: 5,
        }
    }
}

/// How absolute and repo-relative scoring are blended.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScoringConfig {
    /// Share of the absolute part; the rest is repo-relative. Default 0.6.
    pub absolute_share: f64,
}

impl Default for ScoringConfig {
    fn default() -> Self {
        Self { absolute_share: 0.6 }
    }
}

/// Optional coverage input.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CoverageConfig {
    /// Path to an `lcov.info`, relative to the scanned root.
    pub lcov: Option<String>,
}

impl Config {
    /// Loads `path`, or `<root>/deadhand.toml` if `path` is `None` and the file exists.
    pub fn load(root: &Path, path: Option<&Path>) -> Result<Config, Error> {
        let file = match path {
            Some(p) => p.to_path_buf(),
            None => {
                let p = root.join("deadhand.toml");
                if !p.is_file() {
                    return Ok(Config::default());
                }
                p
            }
        };
        let text = std::fs::read_to_string(&file).map_err(|e| Error::Config(format!("{}: {e}", file.display())))?;
        Config::from_toml(&text).map_err(|e| Error::Config(format!("{}: {e}", file.display())))
    }

    /// Parses TOML, merging layer paths over the defaults.
    pub fn from_toml(text: &str) -> Result<Config, String> {
        let mut cfg: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut paths = LayerConfig::default().paths;
        paths.extend(std::mem::take(&mut cfg.layers.paths));
        cfg.layers.paths = paths;
        if !(0.0..=1.0).contains(&cfg.scoring.absolute_share) {
            return Err("scoring.absolute_share must be between 0 and 1".into());
        }
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_is_default() {
        let c = Config::from_toml("").unwrap();
        assert_eq!(c.thresholds.cognitive_complexity, 15);
        assert_eq!(c.layers.order.len(), 5);
    }

    #[test]
    fn partial_override_keeps_other_defaults() {
        let c = Config::from_toml(
            "[weights]\ncognitive_load = 0.5\n[thresholds]\nmax_nesting = 3\n[layers.paths]\nservice = [\"core\"]\n",
        )
        .unwrap();
        assert_eq!(c.weights.cognitive_load, 0.5);
        assert_eq!(c.weights.entanglement, 0.20);
        assert_eq!(c.thresholds.max_nesting, 3);
        assert_eq!(c.layers.paths["service"], vec!["core"]);
        assert!(c.layers.paths.contains_key("data"));
    }

    #[test]
    fn unknown_key_is_rejected() {
        assert!(Config::from_toml("bogus = 1").is_err());
    }
}

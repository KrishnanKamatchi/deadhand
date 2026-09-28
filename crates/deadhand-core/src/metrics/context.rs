//! Context Depth: how much of the repo must be understood before changing this module?
//!
//! Raw value: Σ 1/distance over the transitive value-import closure (direct deps weigh most).
//! Absolute score: the lower of linear-on-that-sum and linear-on-max-depth, because a long
//! chain of single imports is cheap by Σ 1/d but still has to be read end to end. Also records closure size, max depth, and distinct
//! layers and top-level directories in the closure.

use std::collections::BTreeSet;

use super::{fmt_num, linear, values, Inputs, MetricOutput, ModuleValue};
use crate::evidence::{Evidence, MetricKind, Severity};

/// Longest directory prefix shared by every module (e.g. `src/`).
fn common_dir(paths: &[&str]) -> usize {
    let Some(first) = paths.first() else { return 0 };
    let mut prefix: &str = first.rsplit_once('/').map_or("", |(d, _)| d);
    for p in paths {
        while !prefix.is_empty() && !p.strip_prefix(prefix).is_some_and(|rest| rest.starts_with('/')) {
            prefix = prefix.rsplit_once('/').map_or("", |(d, _)| d);
        }
    }
    if prefix.is_empty() { 0 } else { prefix.len() + 1 }
}

pub fn run(inp: &Inputs<'_>) -> MetricOutput {
    let kind = MetricKind::ContextDepth;
    let t = &inp.cfg.thresholds;
    let mut out = MetricOutput::new(kind, inp.modules.len());

    let paths: Vec<&str> = inp.modules.iter().map(|m| m.path.as_str()).collect();
    let strip = common_dir(&paths);
    let top_dir = |i: usize| -> &str {
        let rest = &inp.modules[i].path[strip..];
        rest.split_once('/').map_or(".", |(d, _)| d)
    };

    let mut wide = 0usize;
    for (i, m) in inp.modules.iter().enumerate() {
        let closure = &inp.dependencies[i];
        let weighted: f64 = closure.iter().map(|&(_, d)| 1.0 / d as f64).sum();
        let depth = closure.iter().map(|&(_, d)| d).max().unwrap_or(0);
        let layers: BTreeSet<&str> = closure.iter().filter_map(|&(j, _)| inp.layer(j)).collect();
        let dirs: BTreeSet<&str> = closure.iter().map(|&(j, _)| top_dir(j)).collect();

        let deep = f64::from(depth) > t.context_depth_good;
        if weighted > t.context_good || deep || layers.len() >= t.context_layers {
            let severity = if weighted > t.context_bad || f64::from(depth) >= t.context_depth_bad || layers.len() >= t.context_layers {
                Severity::Warn
            } else {
                Severity::Info
            };
            let layer_list = layers.iter().copied().collect::<Vec<_>>().join(", ");
            out.evidence.push(Evidence {
                metric: kind,
                severity,
                path: m.path.clone(),
                span: None,
                key: "closure".into(),
                message: format!(
                    "Depends on {} modules (weighted {}), {} layers ({}), max depth {}",
                    closure.len(),
                    fmt_num(weighted),
                    layers.len(),
                    if layer_list.is_empty() { "none" } else { &layer_list },
                    depth
                ),
                values: vec![
                    ("closure_size".into(), closure.len() as f64),
                    ("weighted".into(), weighted),
                    ("layers".into(), layers.len() as f64),
                    ("max_depth".into(), depth as f64),
                ],
            });
        }
        if layers.len() >= t.context_layers && !m.is_test {
            wide += 1;
        }

        out.modules[i] = ModuleValue {
            raw: weighted,
            abs: linear(weighted, t.context_good, t.context_bad).min(linear(f64::from(depth), t.context_depth_good, t.context_depth_bad)),
            values: values([
                ("closure_size", closure.len() as f64),
                ("closure_weighted", weighted),
                ("max_depth", depth as f64),
                ("layers_in_closure", layers.len() as f64),
                ("top_dirs_in_closure", dirs.len() as f64),
            ]),
        };
    }
    out.finding(Severity::Warn, wide, format!("{wide} modules depend transitively on {}+ architectural layers", t.context_layers));
    out
}

#[cfg(test)]
mod tests {
    use super::common_dir;

    #[test]
    fn common_prefix() {
        assert_eq!(common_dir(&["src/a/x.ts", "src/b/y.ts"]), 4);
        assert_eq!(common_dir(&["src/a/x.ts", "lib/y.ts"]), 0);
        assert_eq!(common_dir(&["src/a/x.ts", "src/ab/y.ts"]), 4);
    }
}

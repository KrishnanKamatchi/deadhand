//! Cognitive Load: can a person follow the logic?
//!
//! Raw value: cognitive complexity per 100 LOC (min 20 LOC, so tiny files are not amplified).
//! Absolute score: linear on that density, capped by the worst function: a module whose worst
//! function has cognitive `c` above the threshold `t` scores at most `100·t/c`.
//! Flags functions above the threshold, and above the repo p90 when that is at least `t/2`.

use super::{fmt_num, linear, percentile, values, Inputs, MetricOutput, ModuleValue};
use crate::evidence::{Evidence, MetricKind, Severity};

pub fn run(inp: &Inputs<'_>) -> MetricOutput {
    let kind = MetricKind::CognitiveLoad;
    let t = &inp.cfg.thresholds;
    let threshold = t.cognitive_complexity as f64;
    let mut out = MetricOutput::new(kind, inp.modules.len());

    let all: Vec<f64> = inp.population().flat_map(|i| inp.modules[i].functions.iter().map(|f| f.cognitive as f64)).collect();
    let p90 = percentile(&all, 0.9);
    let baseline = if p90 >= threshold / 2.0 { p90.min(threshold) } else { threshold };

    let mut flagged = 0usize;
    for (i, m) in inp.modules.iter().enumerate() {
        let total = m.total_cognitive() as f64;
        let density = total / (m.loc.max(20) as f64) * 100.0;
        let worst = m.functions.iter().map(|f| f.cognitive).max().unwrap_or(0) as f64;
        let cap = if worst > threshold { 100.0 * threshold / worst } else { 100.0 };
        let mut over = 0.0;

        for f in &m.functions {
            let c = f.cognitive as f64;
            if c <= baseline {
                continue;
            }
            over += 1.0;
            let severity = if c > 2.0 * threshold {
                Severity::High
            } else if c > threshold {
                Severity::Warn
            } else {
                Severity::Info
            };
            out.evidence.push(Evidence {
                metric: kind,
                severity,
                path: m.path.clone(),
                span: Some(f.span),
                key: format!("function:{}", f.name),
                message: format!(
                    "`{}` has cognitive complexity {} (threshold {}, repo p90 {})",
                    f.name,
                    f.cognitive,
                    t.cognitive_complexity,
                    fmt_num(p90)
                ),
                values: vec![("cognitive".into(), c), ("threshold".into(), threshold), ("repo_p90".into(), p90)],
            });
        }
        if !m.is_test {
            flagged += over as usize;
        }

        out.modules[i] = ModuleValue {
            raw: density,
            abs: linear(density, t.cognitive_density_good, t.cognitive_density_bad).min(cap),
            values: values([
                ("cognitive_total", total),
                ("cognitive_per_100_loc", density),
                ("max_function_cognitive", worst),
                ("functions_over_baseline", over),
            ]),
        };
    }
    out.finding(Severity::Warn, flagged, format!("{flagged} functions exceed the repo cognitive complexity baseline ({})", fmt_num(baseline)));
    out
}

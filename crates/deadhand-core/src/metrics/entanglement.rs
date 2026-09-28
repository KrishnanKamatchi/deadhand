//! Entanglement: what is this module tied to?
//!
//! Raw value: weighted fan-out (value imports + 0.5 × type-only imports), plus 10 + cycle size
//! if the module is in a cycle, plus 8 for shared-module abuse.
//! Absolute score: linear on weighted fan-out, minus 30 (+5 per extra member, max 50) for a
//! cycle, minus 25 for a `shared` module importing from `service` or `data`.

use super::{fmt_num, linear, median, values, Inputs, MetricOutput, ModuleValue};
use crate::evidence::{Evidence, MetricKind, Severity};

pub fn run(inp: &Inputs<'_>) -> MetricOutput {
    let kind = MetricKind::Entanglement;
    let t = &inp.cfg.thresholds;
    let g = inp.graph;
    let mut out = MetricOutput::new(kind, inp.modules.len());

    let fan_outs: Vec<f64> = inp.population().map(|i| g.fan_out(i).0 as f64).collect();
    let fan_out_median = median(&fan_outs);

    let (mut in_cycles, mut abusers) = (0usize, 0usize);
    for (i, m) in inp.modules.iter().enumerate() {
        let (vo, to) = g.fan_out(i);
        let (vi, ti) = g.fan_in(i);
        let weighted = vo as f64 + 0.5 * to as f64;
        let (fan_in, fan_out) = ((vi + ti) as f64, (vo + to) as f64);
        let instability = if fan_in + fan_out > 0.0 { fan_out / (fan_in + fan_out) } else { 0.0 };
        let mut abs = linear(weighted, t.fan_out_good, t.fan_out_bad);
        let mut raw = weighted;

        if weighted > t.fan_out_good {
            out.evidence.push(Evidence {
                metric: kind,
                severity: if weighted > t.fan_out_bad { Severity::High } else { Severity::Warn },
                path: m.path.clone(),
                span: None,
                key: "fan-out".into(),
                message: format!("Imports {} internal modules (repo median {})", vo + to, fmt_num(fan_out_median)),
                values: vec![("fan_out".into(), fan_out), ("repo_median".into(), fan_out_median)],
            });
        }

        let cycle_size = g.cycle_of[i].map_or(0, |c| g.cycles[c].len());
        if let Some(c) = g.cycle_of[i] {
            abs -= (30.0 + 5.0 * (cycle_size as f64 - 2.0)).min(50.0);
            raw += 10.0 + cycle_size as f64;
            let others: Vec<&str> =
                g.cycles[c].iter().filter(|&&j| j != i).map(|&j| inp.modules[j].path.as_str()).collect();
            let shown = others.iter().take(3).copied().collect::<Vec<_>>().join(", ");
            let more = if others.len() > 3 { format!(" and {} more", others.len() - 3) } else { String::new() };
            out.evidence.push(Evidence {
                metric: kind,
                severity: Severity::High,
                path: m.path.clone(),
                span: None,
                key: "cycle".into(),
                message: format!("In an import cycle of {cycle_size} modules with {shown}{more}"),
                values: vec![("cycle_size".into(), cycle_size as f64)],
            });
            if !m.is_test {
                in_cycles += 1;
            }
        }

        let abuse = inp.layer(i) == Some("shared")
            && g.out[i].iter().any(|e| matches!(inp.layer(e.to), Some("service" | "data")));
        if abuse {
            abs -= 25.0;
            raw += 8.0;
            let targets: Vec<String> = g.out[i]
                .iter()
                .filter_map(|e| {
                    inp.layer(e.to)
                        .filter(|l| matches!(*l, "service" | "data"))
                        .map(|l| format!("{} ({l})", inp.modules[e.to].path))
                })
                .collect();
            out.evidence.push(Evidence {
                metric: kind,
                severity: if fan_in as usize >= t.critical_fan_in { Severity::High } else { Severity::Warn },
                path: m.path.clone(),
                span: None,
                key: "shared-imports-upper-layer".into(),
                message: format!("Shared module with {} dependents imports {}", fan_in, targets.join(", ")),
                values: vec![("fan_in".into(), fan_in)],
            });
            if !m.is_test {
                abusers += 1;
            }
        }

        out.modules[i] = ModuleValue {
            raw,
            abs: abs.max(0.0),
            values: values([
                ("fan_in", fan_in),
                ("fan_out", fan_out),
                ("fan_out_weighted", weighted),
                ("instability", instability),
                ("cycle_size", cycle_size as f64),
                ("shared_abuse", f64::from(u8::from(abuse))),
            ]),
        };
    }
    out.finding(Severity::High, in_cycles, format!("{in_cycles} modules participate in circular dependencies"));
    out.finding(Severity::Warn, abusers, format!("{abusers} shared modules import from the service or data layer"));
    out
}

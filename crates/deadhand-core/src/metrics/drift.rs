//! Pattern Drift: does this module behave like its peers?
//!
//! Peer group: same layer if it has ≥ 5 modules, else same directory (≥ 5), else the whole repo.
//! For each feature, a robust z-score `(x − median) / max(1.4826·MAD, 0.25·|median|, floor)`, where
//! `floor` is the smallest difference that matters for that feature (e.g. 20 LOC, 2 imports).
//! Function features are skipped for modules without functions.
//! Drift = Σ min(|z|, 6) over features with |z| > 2, divided by the feature count.
//! Raw value = drift + 1 per layer-order violation. Absolute score: linear on raw.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::{fmt_num, linear, mad, median, values, Inputs, MetricOutput, ModuleValue};
use crate::evidence::{Evidence, MetricKind, Severity};
use crate::layers::{violation, Violation};
use crate::model::{Casing, ImportTarget, ModuleFacts};

const MIN_PEERS: usize = 5;
const Z_FLAG: f64 = 2.0;
const Z_CAP: f64 = 6.0;
const NUMERIC: [&str; 7] = ["fan_out", "exports", "layers_crossed", "loc", "median_function_loc", "max_nesting", "external_packages"];
/// Smallest meaningful difference per feature, so near-identical peers do not produce huge z-scores.
const FLOOR: [f64; 7] = [2.0, 2.0, 1.0, 20.0, 5.0, 1.0, 2.0];
/// Per-group robust statistics: medians, scales and the dominant naming convention.
type GroupStats = ([f64; 7], [f64; 7], Option<Casing>);
const FEATURES: f64 = NUMERIC.len() as f64 + 1.0; // + naming convention

fn dominant_casing(m: &ModuleFacts) -> Option<Casing> {
    let names = m.symbols.iter().map(|s| &s.name).chain(m.functions.iter().flat_map(|f| &f.identifiers));
    let (mut camel, mut snake) = (0, 0);
    for n in names {
        match Casing::of(n) {
            Casing::Camel => camel += 1,
            Casing::Snake => snake += 1,
            _ => {}
        }
    }
    match camel.cmp(&snake) {
        std::cmp::Ordering::Greater => Some(Casing::Camel),
        std::cmp::Ordering::Less => Some(Casing::Snake),
        std::cmp::Ordering::Equal => None,
    }
}

fn casing_name(c: Casing) -> &'static str {
    if c == Casing::Snake { "snake_case" } else { "camelCase" }
}

fn features(inp: &Inputs<'_>, i: usize) -> [Option<f64>; 7] {
    let m = &inp.modules[i];
    let (vo, to) = inp.graph.fan_out(i);
    let own = inp.layer(i);
    let crossed: BTreeSet<&str> = inp.graph.out[i].iter().filter_map(|e| inp.layer(e.to)).filter(|l| Some(*l) != own).collect();
    let fn_locs: Vec<f64> = m.functions.iter().map(|f| f.loc as f64).collect();
    let has_fns = !m.functions.is_empty();
    [
        Some((vo + to) as f64),
        Some(m.exports as f64),
        Some(crossed.len() as f64),
        Some(m.loc as f64),
        has_fns.then(|| median(&fn_locs)),
        m.functions.iter().map(|f| f.max_nesting as f64).reduce(f64::max),
        Some(m.external_packages().len() as f64),
    ]
}

fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

pub fn run(inp: &Inputs<'_>) -> MetricOutput {
    let kind = MetricKind::PatternDrift;
    let t = &inp.cfg.thresholds;
    let mut out = MetricOutput::new(kind, inp.modules.len());
    let pop: Vec<usize> = inp.population().collect();
    let feats: HashMap<usize, [Option<f64>; 7]> = pop.iter().map(|&i| (i, features(inp, i))).collect();
    let casing: HashMap<usize, Option<Casing>> = pop.iter().map(|&i| (i, dominant_casing(&inp.modules[i]))).collect();

    let mut by_layer: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    let mut by_dir: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for &i in &pop {
        if let Some(l) = inp.layer(i) {
            by_layer.entry(l).or_default().push(i);
        }
        by_dir.entry(parent_dir(&inp.modules[i].path)).or_default().push(i);
    }
    let index: HashMap<&str, usize> = inp.modules.iter().enumerate().map(|(i, m)| (m.path.as_str(), i)).collect();

    let mut stats_cache: HashMap<String, GroupStats> = HashMap::new();
    let (mut drifting, mut violations_total) = (0usize, 0usize);

    for &i in &pop {
        let m = &inp.modules[i];
        let (label, group): (String, &[usize]) = match inp.layer(i).and_then(|l| by_layer.get(l).map(|g| (l, g))) {
            Some((l, g)) if g.len() >= MIN_PEERS => (format!("layer {l}"), g),
            _ => match by_dir.get(parent_dir(&m.path)) {
                Some(g) if g.len() >= MIN_PEERS => (format!("dir {}/", parent_dir(&m.path)), g),
                _ => ("repo".to_string(), &pop),
            },
        };
        let (medians, scales, group_casing) = *stats_cache
            .entry(label.clone())
            .or_insert_with(|| {
                let mut med = [0.0; 7];
                let mut scale = [1.0; 7];
                for f in 0..NUMERIC.len() {
                    let col: Vec<f64> = group.iter().filter_map(|j| feats[j][f]).collect();
                    med[f] = median(&col);
                    scale[f] = (1.4826 * mad(&col, med[f])).max(0.25 * med[f].abs()).max(FLOOR[f]);
                }
                let mut counts: BTreeMap<Casing, usize> = BTreeMap::new();
                for c in group.iter().filter_map(|j| casing[j]) {
                    *counts.entry(c).or_default() += 1;
                }
                let top = counts.iter().max_by_key(|(_, n)| **n).map(|(c, _)| *c);
                (med, scale, top)
            });

        let x = feats[&i];
        let mut sum = 0.0;
        let mut parts = Vec::new();
        let mut vals = Vec::new();
        for f in 0..NUMERIC.len() {
            let Some(xf) = x[f] else { continue };
            let z = (xf - medians[f]) / scales[f];
            if z.abs() > Z_FLAG {
                sum += z.abs().min(Z_CAP);
                parts.push(format!("{} {} (peer median {})", NUMERIC[f].replace('_', " "), fmt_num(xf), fmt_num(medians[f])));
                vals.push((NUMERIC[f].to_string(), xf));
                vals.push((format!("{}_peer_median", NUMERIC[f]), medians[f]));
            }
        }
        if let (Some(own), Some(peer)) = (casing[&i], group_casing) {
            if own != peer {
                sum += 3.0;
                parts.push(format!("naming {} (peers use {})", casing_name(own), casing_name(peer)));
            }
        }
        let drift = sum / FEATURES;
        if !parts.is_empty() {
            drifting += 1;
            out.evidence.push(Evidence {
                metric: kind,
                severity: if drift >= t.drift_bad / 2.0 { Severity::Warn } else { Severity::Info },
                path: m.path.clone(),
                span: None,
                key: "peer-deviation".into(),
                message: format!("Unlike its peers ({label}, {} modules): {}", group.len(), parts.join("; ")),
                values: vals,
            });
        }

        let mut violations = 0usize;
        if let Some(from) = inp.layer(i) {
            let mut seen = BTreeSet::new();
            for imp in &m.imports {
                let ImportTarget::Internal(p) = &imp.target else { continue };
                if imp.kind.is_type_only() {
                    continue;
                }
                let Some(&j) = index.get(p.as_str()) else { continue };
                let Some(to) = inp.layer(j) else { continue };
                let Some(v) = violation(from, to, &inp.cfg.layers.order) else { continue };
                if !seen.insert(j) {
                    continue;
                }
                violations += 1;
                let why = match v {
                    Violation::Inverted => format!("{from} layer imports {to}, which sits above it"),
                    Violation::SkipsService => format!("{from} layer imports {to} directly, skipping service"),
                };
                out.evidence.push(Evidence {
                    metric: kind,
                    severity: Severity::Warn,
                    path: m.path.clone(),
                    span: Some(crate::model::Span { start_line: imp.line, start_col: 1, end_line: imp.line, end_col: 1 }),
                    key: format!("layer-violation:{p}"),
                    message: format!("Imports {p}: {why}"),
                    values: vec![],
                });
            }
        }
        violations_total += violations;

        let raw = drift + violations as f64;
        let mut v = values([("drift", drift), ("layer_violations", violations as f64), ("peer_group_size", group.len() as f64)]);
        for f in 0..NUMERIC.len() {
            if let Some(xf) = x[f] {
                v.insert(NUMERIC[f].to_string(), xf);
            }
        }
        out.modules[i] = ModuleValue { raw, abs: linear(raw, t.drift_good, t.drift_bad), values: v };
    }
    for (i, m) in inp.modules.iter().enumerate() {
        if m.is_test {
            out.modules[i].abs = 100.0;
        }
    }
    out.finding(Severity::Warn, violations_total, format!("{violations_total} imports break the layer order"));
    out.finding(Severity::Info, drifting, format!("{drifting} modules deviate from their peer group"));
    out
}

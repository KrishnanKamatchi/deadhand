//! Readability: can a person scan it?
//!
//! Penalty points per module (raw value; 0 = clean):
//! - +1 per function longer than max(3× repo median function LOC, 40)
//! - +1 per function with nesting ≥ `max_nesting`
//! - +1 per function with ≥ `max_params` parameters
//! - +0.25 per vague identifier, capped at 2
//! - +1 if camelCase and snake_case are mixed for the same kind of symbol
//! - +1 if the file is longer than max(3× repo median file LOC, 300)

use std::collections::BTreeSet;

use super::{fmt_num, linear, median, values, Inputs, MetricOutput, ModuleValue};
use crate::evidence::{Evidence, MetricKind, Severity};
use crate::model::{Casing, ModuleFacts, SymbolKind};

const VAGUE: &[&str] = &[
    "data", "obj", "tmp", "temp", "foo", "bar", "baz", "stuff", "thing", "things", "info", "val", "arr", "str", "num", "ret",
];

/// Vague name: single letter, letter+digits (`x2`), or a generic placeholder word.
pub fn is_vague(name: &str) -> bool {
    let bytes = name.as_bytes();
    match bytes {
        [] | [b'_'] => false,
        [c] => c.is_ascii_alphabetic(),
        [c, rest @ ..] if rest.len() <= 2 && c.is_ascii_alphabetic() && rest.iter().all(u8::is_ascii_digit) => true,
        _ => VAGUE.contains(&name),
    }
}

/// Returns `(camel_examples, snake_examples)` if a file mixes the two conventions.
fn mixed_naming(m: &ModuleFacts) -> Option<(String, String)> {
    let names = m
        .symbols
        .iter()
        .filter(|s| s.kind != SymbolKind::Class)
        .map(|s| s.name.as_str())
        .chain(m.functions.iter().flat_map(|f| f.identifiers.iter().map(String::as_str)));
    let (mut camel, mut snake) = (BTreeSet::new(), BTreeSet::new());
    for n in names {
        match Casing::of(n) {
            Casing::Camel => camel.insert(n),
            Casing::Snake => snake.insert(n),
            _ => false,
        };
    }
    let first = |s: &BTreeSet<&str>| s.iter().take(2).copied().collect::<Vec<_>>().join(", ");
    (!camel.is_empty() && !snake.is_empty()).then(|| (first(&camel), first(&snake)))
}

pub fn run(inp: &Inputs<'_>) -> MetricOutput {
    let kind = MetricKind::Readability;
    let t = &inp.cfg.thresholds;
    let mut out = MetricOutput::new(kind, inp.modules.len());

    let fn_locs: Vec<f64> = inp.population().flat_map(|i| inp.modules[i].functions.iter().map(|f| f.loc as f64)).collect();
    let file_locs: Vec<f64> = inp.population().map(|i| inp.modules[i].loc as f64).collect();
    let (fn_median, file_median) = (median(&fn_locs), median(&file_locs));
    let long_fn = (3.0 * fn_median).max(40.0);
    let long_file = (3.0 * file_median).max(300.0);

    let (mut deep_total, mut params_total) = (0usize, 0usize);
    for (i, m) in inp.modules.iter().enumerate() {
        let mut points = 0.0;
        let (mut long, mut deep, mut params, mut vague_count) = (0.0, 0.0, 0.0, 0.0);
        let mut push = |severity, span, key: String, message: String, vals: Vec<(String, f64)>| {
            out.evidence.push(Evidence { metric: kind, severity, path: m.path.clone(), span, key, message, values: vals });
        };

        for f in &m.functions {
            if f.loc as f64 > long_fn {
                long += 1.0;
                push(
                    Severity::Info,
                    Some(f.span),
                    format!("long-function:{}", f.name),
                    format!("`{}` is {} lines (repo median {})", f.name, f.loc, fmt_num(fn_median)),
                    vec![("loc".into(), f.loc as f64), ("repo_median".into(), fn_median)],
                );
            }
            if f.max_nesting >= t.max_nesting {
                deep += 1.0;
                push(
                    Severity::Warn,
                    Some(f.span),
                    format!("deep-nesting:{}", f.name),
                    format!("`{}` nests {} levels deep (threshold {})", f.name, f.max_nesting, t.max_nesting),
                    vec![("max_nesting".into(), f.max_nesting as f64), ("threshold".into(), t.max_nesting as f64)],
                );
            }
            if f.params >= t.max_params {
                params += 1.0;
                push(
                    Severity::Info,
                    Some(f.span),
                    format!("many-params:{}", f.name),
                    format!("`{}` takes {} parameters (threshold {})", f.name, f.params, t.max_params),
                    vec![("params".into(), f.params as f64), ("threshold".into(), t.max_params as f64)],
                );
            }
            let vague: BTreeSet<&str> = f.identifiers.iter().map(String::as_str).filter(|n| is_vague(n)).collect();
            if !vague.is_empty() {
                vague_count += vague.len() as f64;
                let list = vague.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ");
                push(
                    Severity::Info,
                    Some(f.span),
                    format!("vague-names:{}", f.name),
                    format!("`{}` declares vague names: {list}", f.name),
                    vec![("vague_names".into(), vague.len() as f64)],
                );
            }
        }
        points += long + deep + params + (vague_count * 0.25).min(2.0);

        let mixed = mixed_naming(m);
        if let Some((camel, snake)) = &mixed {
            points += 1.0;
            push(
                Severity::Info,
                None,
                "mixed-naming".into(),
                format!("Mixes camelCase ({camel}) and snake_case ({snake})"),
                vec![],
            );
        }
        let long_file_flag = m.loc as f64 > long_file;
        if long_file_flag {
            points += 1.0;
            push(
                Severity::Warn,
                None,
                "long-file".into(),
                format!("File is {} lines (repo median {})", m.loc, fmt_num(file_median)),
                vec![("loc".into(), m.loc as f64), ("repo_median".into(), file_median)],
            );
        }
        if !m.is_test {
            deep_total += deep as usize;
            params_total += params as usize;
        }

        out.modules[i] = ModuleValue {
            raw: points,
            abs: linear(points, t.readability_good, t.readability_bad),
            values: values([
                ("penalty_points", points),
                ("long_functions", long),
                ("deeply_nested_functions", deep),
                ("many_param_functions", params),
                ("vague_names", vague_count),
                ("mixed_naming", f64::from(u8::from(mixed.is_some()))),
                ("loc", m.loc as f64),
            ]),
        };
    }
    out.finding(Severity::Warn, deep_total, format!("{deep_total} functions nest {}+ levels deep", t.max_nesting));
    out.finding(Severity::Info, params_total, format!("{params_total} functions take {}+ parameters", t.max_params));
    out
}

#[cfg(test)]
mod tests {
    use super::is_vague;

    #[test]
    fn vague_names() {
        for n in ["a", "x2", "tmp", "data", "obj"] {
            assert!(is_vague(n), "{n}");
        }
        for n in ["_", "id", "user", "total", "i18n", "dataset"] {
            assert!(!is_vague(n), "{n}");
        }
    }
}

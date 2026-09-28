//! End-to-end checks against the hand-written fixture repos.

use std::path::PathBuf;

use deadhand_core::config::Config;
use deadhand_core::evidence::MetricKind;
use deadhand_core::git::NoGit;
use deadhand_core::model::ImportTarget;
use deadhand_core::report::RepoReport;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name)
}

fn scan(name: &str) -> RepoReport {
    deadhand_core::analyze_with(&fixture(name), &Config::default(), &NoGit).expect("scan")
}

fn score(r: &RepoReport, k: MetricKind) -> f64 {
    r.metric(k).and_then(|m| m.score).expect("score")
}

#[test]
fn every_fixture_parses_cleanly() {
    for name in ["clean", "cycles", "spaghetti", "deep-context", "drift", "aliases"] {
        let r = scan(name);
        assert!(r.parse_errors.is_empty(), "{name}: {:?}", r.parse_errors);
        assert!(r.summary.files > 0, "{name}");
    }
}

#[test]
fn cycles_reports_exact_groups() {
    let r = scan("cycles");
    assert_eq!(r.cycles, vec![vec!["src/a.ts", "src/b.ts", "src/c.ts"], vec!["src/d.ts", "src/e.ts"]]);
}

#[test]
fn aliases_resolve() {
    let r = scan("aliases");
    let index = r.module("src/index.ts").unwrap();
    let targets: Vec<_> = index.imports.iter().map(|i| (i.specifier.as_str(), &i.target)).collect();
    let internal = |p: &str| ImportTarget::Internal(p.to_string());
    assert_eq!(targets[0], ("@app/math/add", &internal("src/math/add.ts")));
    assert_eq!(targets[1], ("~lib/clamp", &internal("src/lib/clamp.ts")));
    assert_eq!(targets[2], ("./meta.js", &internal("src/meta.ts")));
    assert_eq!(targets[3], ("./helpers", &internal("src/helpers/index.ts")));
    assert_eq!(targets[4], ("react", &ImportTarget::External("react".into())));
}

#[test]
fn drift_flags_the_outlier_only() {
    let r = scan("drift");
    let flagged: Vec<_> = r
        .evidence
        .iter()
        .filter(|e| e.metric == MetricKind::PatternDrift && e.key == "peer-deviation")
        .map(|e| e.path.as_str())
        .collect();
    assert_eq!(flagged, vec!["src/services/legacyReport.ts"]);
}

#[test]
fn spaghetti_flags_layer_violations_and_shared_abuse() {
    let r = scan("spaghetti");
    let mut violations: Vec<_> = r
        .evidence
        .iter()
        .filter(|e| e.key.starts_with("layer-violation:"))
        .map(|e| (e.path.as_str(), e.key.as_str()))
        .collect();
    violations.sort();
    assert_eq!(
        violations,
        vec![
            ("src/components/OrderList.tsx", "layer-violation:src/db/client.ts"),
            ("src/db/client.ts", "layer-violation:src/routes/orders.ts"),
            ("src/routes/orders.ts", "layer-violation:src/db/client.ts"),
        ]
    );
    assert!(r.evidence.iter().any(|e| e.path == "src/utils/helpers.ts" && e.key == "shared-imports-upper-layer"));
}

#[test]
fn deep_context_flags_long_chain() {
    let r = scan("deep-context");
    let entry = r.module("src/entry.ts").unwrap();
    assert_eq!(entry.raw[&MetricKind::ContextDepth]["max_depth"], 12.0);
    assert!(score(&r, MetricKind::ContextDepth) < 80.0);
}

#[test]
fn clean_scores_clearly_higher_than_spaghetti() {
    let (clean, spaghetti) = (scan("clean"), scan("spaghetti"));
    assert!(clean.maintainability >= 90.0, "clean {}", clean.maintainability);
    assert!(
        clean.maintainability - spaghetti.maintainability >= 25.0,
        "clean {} spaghetti {}",
        clean.maintainability,
        spaghetti.maintainability
    );
}

#[test]
fn unavailable_git_redistributes_weight() {
    let r = scan("clean");
    let orphan = r.metric(MetricKind::OrphanedCode).unwrap();
    assert!(!orphan.available && orphan.score.is_none() && orphan.weight == 0.0);
    let total: f64 = r.metrics.iter().map(|m| m.weight).sum();
    assert!((total - 1.0).abs() < 1e-9);
    assert!(!r.notes.is_empty());
}

#[test]
fn scans_are_byte_identical() {
    for name in ["spaghetti", "drift", "aliases"] {
        let a = serde_json::to_string(&scan(name)).unwrap();
        let b = serde_json::to_string(&scan(name)).unwrap();
        assert_eq!(a, b, "{name}");
    }
}

#[test]
fn json_snapshots() {
    for name in ["clean", "cycles", "spaghetti", "deep-context", "drift", "aliases"] {
        insta::assert_json_snapshot!(name, serde_json::to_value(scan(name)).unwrap());
    }
}

//! Deadhand core: measures how hard a JS/TS repository is for a human to understand and change.
//!
//! Pipeline: discover → parse (parallel) → import graph → git → layers → metrics → scoring → report.

pub mod config;
pub mod coverage;
pub mod diff;
pub mod discover;
pub mod evidence;
pub mod git;
pub mod graph;
pub mod layers;
pub mod metrics;
pub mod model;
pub mod parse;
pub mod report;
pub mod resolve;
pub mod scoring;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use config::Config;
use evidence::sort_evidence;
use git::{GitCli, GitSource};
use graph::ImportGraph;
use model::ModuleFacts;
use report::{MetricSummary, ModuleReport, ParseError, RepoReport, Summary, SCHEMA_VERSION};
use resolve::ModuleResolver;

/// Library errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("config error: {0}")]
    Config(String),
    #[error("i/o error: {0}")]
    Io(String),
    #[error("git error: {0}")]
    Git(String),
}

/// Per-file facts for a whole repository.
#[derive(Debug, Clone)]
pub struct ParsedRepo {
    /// Canonical root directory.
    pub root: PathBuf,
    /// Sorted by path.
    pub modules: Vec<ModuleFacts>,
    /// Files skipped because of an `@generated` header.
    pub generated: Vec<String>,
}

/// Discovers and parses every source file under `root` in parallel.
pub fn parse_repo(root: &Path, cfg: &Config) -> Result<ParsedRepo, Error> {
    let root = root.canonicalize().map_err(|e| Error::Io(format!("{}: {e}", root.display())))?;
    let files = discover::discover(&root, cfg)?;
    let resolver = ModuleResolver::new(&root, files.iter().map(|f| f.rel.clone()));

    let results: Vec<Result<Option<ModuleFacts>, String>> = files
        .par_iter()
        .map(|f| {
            let source = std::fs::read_to_string(&f.abs).map_err(|e| format!("cannot read file: {e}"))?;
            if discover::is_generated(&source) {
                return Ok(None);
            }
            Ok(Some(parse::parse_module(&f.rel, f.is_test, &source, |spec| resolver.resolve(&f.abs, spec))))
        })
        .collect();

    let mut modules = Vec::with_capacity(results.len());
    let mut generated = Vec::new();
    for (file, result) in files.iter().zip(results) {
        match result {
            Ok(Some(m)) => modules.push(m),
            Ok(None) => generated.push(file.rel.clone()),
            Err(msg) => modules.push(ModuleFacts {
                path: file.rel.clone(),
                is_test: file.is_test,
                loc: 0,
                imports: Vec::new(),
                exports: 0,
                top_level_decls: 0,
                functions: Vec::new(),
                top_level_cognitive: 0,
                symbols: Vec::new(),
                parse_errors: vec![msg],
            }),
        }
    }
    Ok(ParsedRepo { root, modules, generated })
}

/// Scans `root` with the system `git` binary for history.
pub fn analyze(root: &Path, cfg: &Config) -> Result<RepoReport, Error> {
    analyze_with(root, cfg, &GitCli)
}

/// Scans `root` using `git` as the history source.
pub fn analyze_with(root: &Path, cfg: &Config, git: &dyn GitSource) -> Result<RepoReport, Error> {
    let parsed = parse_repo(root, cfg)?;
    let modules = &parsed.modules;
    let graph = ImportGraph::build(modules);
    let layers: Vec<Option<String>> = modules.iter().map(|m| layers::detect(&m.path, &cfg.layers)).collect();
    let git_facts = git::collect(&parsed.root, git, cfg.thresholds.churn_days)?;
    let coverage = match &cfg.coverage.lcov {
        Some(p) => {
            let path = parsed.root.join(p);
            let text = std::fs::read_to_string(&path).map_err(|e| Error::Config(format!("coverage.lcov {}: {e}", path.display())))?;
            Some(coverage::parse_lcov(&text, &parsed.root))
        }
        None => None,
    };
    let dependencies: Vec<_> = (0..modules.len()).into_par_iter().map(|i| graph.dependencies(i)).collect();
    let dependents: Vec<_> = (0..modules.len()).into_par_iter().map(|i| graph.dependents(i)).collect();

    let inputs = metrics::Inputs {
        modules,
        graph: &graph,
        layers: &layers,
        git: git_facts.as_ref(),
        coverage: coverage.as_ref(),
        cfg,
        dependencies: &dependencies,
        dependents: &dependents,
    };
    let outputs = metrics::run_all(&inputs);
    let scores = scoring::score(&outputs, modules, cfg);
    Ok(build_report(&parsed, &graph, &layers, git_facts.is_some(), &outputs, &scores))
}

fn build_report(
    parsed: &ParsedRepo,
    graph: &ImportGraph,
    layers: &[Option<String>],
    git_available: bool,
    outputs: &[metrics::MetricOutput],
    scores: &scoring::Scores,
) -> RepoReport {
    let modules = &parsed.modules;
    let mut evidence: Vec<_> = outputs.iter().flat_map(|o| o.evidence.iter().cloned()).collect();
    sort_evidence(&mut evidence);

    let mut findings: Vec<_> = outputs.iter().flat_map(|o| o.findings.iter().cloned()).collect();
    findings.sort_by(|a, b| b.severity.cmp(&a.severity).then_with(|| a.metric.cmp(&b.metric)).then_with(|| a.message.cmp(&b.message)));
    let parse_errors: Vec<ParseError> = modules
        .iter()
        .flat_map(|m| m.parse_errors.iter().map(|e| ParseError { path: m.path.clone(), message: e.clone() }))
        .collect();
    let error_files = modules.iter().filter(|m| !m.is_test && !m.parse_errors.is_empty()).count();
    if error_files > 0 {
        findings.insert(
            0,
            evidence::Finding {
                metric: evidence::MetricKind::Readability,
                severity: evidence::Severity::Warn,
                count: error_files,
                message: format!("{error_files} files have parse errors; their facts may be incomplete"),
            },
        );
    }

    let metrics_summary = outputs
        .iter()
        .zip(&scores.metrics)
        .map(|(o, s)| MetricSummary {
            kind: o.kind,
            label: o.kind.label().to_string(),
            available: o.available,
            score: s.repo,
            weight: s.effective_weight,
            notes: o.notes.clone(),
        })
        .collect();

    let module_reports = modules
        .iter()
        .enumerate()
        .map(|(i, m)| ModuleReport {
            path: m.path.clone(),
            layer: layers[i].clone(),
            is_test: m.is_test,
            loc: m.loc,
            maintainability: scores.module_maintainability[i],
            scores: scores.metrics.iter().filter(|s| s.available).map(|s| (s.kind, s.modules[i])).collect(),
            raw: outputs.iter().filter(|o| o.available).map(|o| (o.kind, o.modules[i].values.clone())).collect::<BTreeMap<_, _>>(),
            functions: m.functions.clone(),
            imports: m.imports.clone(),
        })
        .collect();

    let root_name = parsed.root.file_name().map_or_else(|| ".".to_string(), |n| n.to_string_lossy().into_owned());
    RepoReport {
        schema_version: SCHEMA_VERSION,
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        root_name,
        summary: Summary {
            files: modules.len(),
            test_files: modules.iter().filter(|m| m.is_test).count(),
            loc: modules.iter().map(|m| u64::from(m.loc)).sum(),
            functions: modules.iter().map(|m| m.functions.len()).sum(),
            generated_skipped: parsed.generated.len(),
            git_available,
        },
        maintainability: scores.maintainability,
        metrics: metrics_summary,
        findings,
        notes: scores.redistribution_note.iter().cloned().collect(),
        cycles: graph.cycles.iter().map(|c| c.iter().map(|&i| modules[i].path.clone()).collect()).collect(),
        parse_errors,
        modules: module_reports,
        evidence,
    }
}

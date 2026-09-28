//! Terminal renderer.

use std::fmt::Write;
use std::time::Duration;

use comfy_table::presets::UTF8_HORIZONTAL_ONLY;
use comfy_table::{CellAlignment, Table};
use deadhand_core::diff::DiffReport;
use deadhand_core::evidence::{Evidence, Severity};
use deadhand_core::report::{ModuleReport, RepoReport};
use owo_colors::{OwoColorize, Style};

/// Inner width of the audit box.
const BOX: usize = 46;
const METRIC_WARN: f64 = 50.0;
const HEADLINE_WARN: f64 = 60.0;

struct Paint(bool);

impl Paint {
    fn style(&self, s: &str, style: Style) -> String {
        if self.0 {
            s.style(style).to_string()
        } else {
            s.to_string()
        }
    }
    fn score(&self, s: &str, score: f64) -> String {
        let style = if score < METRIC_WARN {
            Style::new().red().bold()
        } else if score < 75.0 {
            Style::new().yellow()
        } else {
            Style::new().green()
        };
        self.style(s, style)
    }
    fn dim(&self, s: &str) -> String {
        self.style(s, Style::new().dimmed())
    }
    fn bold(&self, s: &str) -> String {
        self.style(s, Style::new().bold())
    }
    fn mark(&self, severity: Severity) -> String {
        match severity {
            Severity::High => self.style("high", Style::new().red().bold()),
            Severity::Warn => self.style("warn", Style::new().yellow()),
            Severity::Info => self.dim("info"),
        }
    }
}

fn round(v: f64) -> i64 {
    v.round() as i64
}

fn box_line(p: &Paint, label: &str, score: Option<f64>, warn_below: f64) -> String {
    let (num, mark, s) = match score {
        Some(s) => (format!("{:>3} / 100", round(s)), if s < warn_below { "!" } else { " " }, s),
        None => ("n/a      ".to_string(), " ", 100.0),
    };
    let label_cell = format!("{label:<32}");
    let body = format!(" {label_cell}{num} {mark}  ");
    debug_assert_eq!(body.chars().count(), BOX);
    let num = if score.is_some() { p.score(&num, s) } else { p.dim(&num) };
    let mark = if mark == "!" { p.style(mark, Style::new().yellow().bold()) } else { mark.to_string() };
    format!("│ {label_cell}{num} {mark}  │\n")
}

/// `deadhand scan` output.
pub fn scan(r: &RepoReport, top: usize, color: bool, elapsed: Option<Duration>) -> String {
    let p = Paint(color);
    let mut o = String::new();
    let rule = "─".repeat(BOX);
    let _ = writeln!(o, "╭{rule}╮");
    let _ = writeln!(o, "│{}│", p.bold(&format!("{:^BOX$}", "DEADHAND AUDIT")));
    let _ = writeln!(o, "│{:BOX$}│", "");
    for m in &r.metrics {
        o.push_str(&box_line(&p, &m.label, m.score, METRIC_WARN));
    }
    let _ = writeln!(o, "│{:BOX$}│", "");
    o.push_str(&box_line(&p, "MAINTAINABILITY", Some(r.maintainability), HEADLINE_WARN));
    let _ = writeln!(o, "╰{rule}╯");

    if !r.findings.is_empty() {
        o.push('\n');
        for f in &r.findings {
            let _ = writeln!(o, "{} {}", p.mark(f.severity), f.message);
        }
    }

    let worst = r.worst(top);
    if !worst.is_empty() && top > 0 {
        let _ = writeln!(o, "\nWorst modules:");
        let width = worst.iter().map(|m| m.path.chars().count()).max().unwrap_or(0).max(24);
        for m in worst {
            let why = m.weakest(2).iter().map(|k| k.label()).collect::<Vec<_>>().join(", ");
            let score = format!("{:>3}", round(m.maintainability));
            let _ = writeln!(o, "  {:<width$}  {}   {}", m.path, p.score(&score, m.maintainability), p.dim(&why));
        }
    }

    let notes: Vec<&String> = r.metrics.iter().flat_map(|m| &m.notes).chain(&r.notes).collect();
    if !notes.is_empty() {
        o.push('\n');
        for n in notes {
            let _ = writeln!(o, "{}", p.dim(&format!("note: {n}")));
        }
    }

    let s = &r.summary;
    let mut footer = format!("{} files ({} tests) · {} LOC · {} functions", s.files, s.test_files, s.loc, s.functions);
    if s.generated_skipped > 0 {
        let _ = write!(footer, " · {} generated skipped", s.generated_skipped);
    }
    if let Some(d) = elapsed {
        let _ = write!(footer, " · {:.2}s", d.as_secs_f64());
    }
    let _ = writeln!(o, "\n{}", p.dim(&footer));
    o
}

fn evidence_line(p: &Paint, e: &Evidence, with_path: bool) -> String {
    let loc = e.span.map_or(String::new(), |s| format!("L{} ", s.start_line));
    let path = if with_path { format!("{}  ", e.path) } else { String::new() };
    format!("  {} {}{}{}  {}\n", p.mark(e.severity), path, p.dim(&loc), e.message, p.dim(e.metric.label()))
}

/// `deadhand explain` output.
pub fn explain(r: &RepoReport, m: &ModuleReport, color: bool) -> String {
    let p = Paint(color);
    let mut o = String::new();
    let layer = m.layer.as_deref().unwrap_or("unknown");
    let test = if m.is_test { ", test" } else { "" };
    let _ = writeln!(o, "{}  {}", p.bold(&m.path), p.dim(&format!("(layer {layer}, {} LOC{test})", m.loc)));
    let _ =
        writeln!(o, "Maintainability {} / 100\n", p.score(&round(m.maintainability).to_string(), m.maintainability));

    let _ = writeln!(o, "Scores:");
    for summary in &r.metrics {
        let Some(score) = m.scores.get(&summary.kind) else {
            let _ = writeln!(o, "  {:<16} {}", summary.label, p.dim("n/a"));
            continue;
        };
        let raw = m.raw.get(&summary.kind).map_or(String::new(), |vals| {
            vals.iter().map(|(k, v)| format!("{k}={}", fmt_raw(*v))).collect::<Vec<_>>().join(" ")
        });
        let _ = writeln!(
            o,
            "  {:<16} {}  {}",
            summary.label,
            p.score(&format!("{:>3}", round(*score)), *score),
            p.dim(&raw)
        );
    }

    if !m.functions.is_empty() {
        let mut fns: Vec<_> = m.functions.iter().collect();
        fns.sort_by(|a, b| b.cognitive.cmp(&a.cognitive).then_with(|| a.span.cmp(&b.span)));
        let mut t = Table::new();
        t.load_style(UTF8_HORIZONTAL_ONLY);
        t.set_header(["function", "line", "loc", "params", "cyclomatic", "cognitive", "nesting"]);
        for f in fns.iter().take(15) {
            t.add_row([
                f.name.clone(),
                f.span.start_line.to_string(),
                f.loc.to_string(),
                f.params.to_string(),
                f.cyclomatic.to_string(),
                f.cognitive.to_string(),
                f.max_nesting.to_string(),
            ]);
        }
        for i in 1..7 {
            if let Some(c) = t.column_mut(i) {
                c.set_cell_alignment(CellAlignment::Right);
            }
        }
        let _ = writeln!(o, "\nFunctions ({} total, most complex first):\n{t}", m.functions.len());
    }

    let ev: Vec<_> = r.evidence_for(&m.path).collect();
    let _ = writeln!(o, "\nEvidence ({}):", ev.len());
    if ev.is_empty() {
        let _ = writeln!(o, "  {}", p.dim("none"));
    }
    for e in ev {
        o.push_str(&evidence_line(&p, e, false));
    }
    o
}

fn fmt_raw(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.2}")
    }
}

/// Change between two scores as displayed (rounded first, so `59 → 60` reads `+1`).
fn change(before: f64, after: f64) -> i64 {
    round(after) - round(before)
}

fn signed(r: i64) -> String {
    if r > 0 {
        format!("+{r}")
    } else {
        r.to_string()
    }
}

/// `deadhand diff` output.
pub fn diff(d: &DiffReport, color: bool) -> String {
    let p = Paint(color);
    let mut o = String::new();
    let _ = writeln!(o, "{}  {} → working tree\n", p.bold("DEADHAND DIFF"), d.revision);
    let delta_style = |r: i64| {
        if r < 0 {
            Style::new().red()
        } else if r > 0 {
            Style::new().green()
        } else {
            Style::new().dimmed()
        }
    };
    let total = change(d.before, d.after);
    let _ = writeln!(
        o,
        "  {:<16} {:>3} → {:>3}  {}",
        "MAINTAINABILITY",
        round(d.before),
        round(d.after),
        p.style(&format!("({})", signed(total)), delta_style(total))
    );
    for m in &d.metrics {
        match (m.before, m.after, m.delta) {
            (Some(b), Some(a), Some(_)) => {
                let c = change(b, a);
                let _ = writeln!(
                    o,
                    "  {:<16} {:>3} → {:>3}  {}",
                    m.label,
                    round(b),
                    round(a),
                    p.style(&format!("({})", signed(c)), delta_style(c))
                );
            }
            _ => {
                let _ = writeln!(o, "  {:<16} {}", m.label, p.dim("n/a"));
            }
        }
    }
    let important = |e: &&Evidence| e.severity >= Severity::Warn;
    let new: Vec<_> = d.new_evidence.iter().filter(important).collect();
    let resolved: Vec<_> = d.resolved_evidence.iter().filter(important).collect();
    let _ = writeln!(o, "\nNew findings ({}):", new.len());
    for e in &new {
        o.push_str(&evidence_line(&p, e, true));
    }
    let _ = writeln!(o, "\nResolved findings ({}):", resolved.len());
    for e in &resolved {
        let loc = e.span.map_or(String::new(), |s| format!("L{} ", s.start_line));
        let _ = writeln!(o, "  {} {}  {}{}", p.style("done", Style::new().green()), e.path, p.dim(&loc), e.message);
    }
    let hidden = d.new_evidence.len() - new.len() + d.resolved_evidence.len() - resolved.len();
    if hidden > 0 {
        let _ = writeln!(o, "\n{}", p.dim(&format!("{hidden} info-level changes not shown (use --format json)")));
    }
    o
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use deadhand_core::config::Config;
    use deadhand_core::git::NoGit;
    use deadhand_core::report::RepoReport;

    fn scan_fixture(name: &str) -> RepoReport {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name);
        deadhand_core::analyze_with(&root, &Config::default(), &NoGit).unwrap()
    }

    #[test]
    fn scan_text_snapshots() {
        for name in ["clean", "cycles", "spaghetti", "deep-context", "drift", "aliases"] {
            insta::assert_snapshot!(format!("scan_{name}"), super::scan(&scan_fixture(name), 5, false, None));
        }
    }

    #[test]
    fn explain_text_snapshot() {
        let r = scan_fixture("spaghetti");
        let m = r.module("src/routes/orders.ts").unwrap();
        insta::assert_snapshot!("explain_spaghetti_route", super::explain(&r, m, false));
    }

    #[test]
    fn box_lines_are_aligned() {
        let out = super::scan(&scan_fixture("spaghetti"), 0, false, None);
        let widths: Vec<usize> = out.lines().take_while(|l| !l.is_empty()).map(|l| l.chars().count()).collect();
        assert!(widths.iter().all(|w| *w == super::BOX + 2), "{widths:?}");
    }
}

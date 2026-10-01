//! `deadhand`: thin CLI over `deadhand-core`. Parses arguments and renders; computes nothing.

mod render;

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use deadhand_core::config::Config;
use deadhand_core::git::{GitCli, GitSource, NoGit};
use deadhand_core::Error;

#[derive(Parser)]
#[command(name = "deadhand", version, about = "Your AI wrote it. Deadhand checks if you can still own it.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scan a JS/TS repository and report how hard it is for a human to maintain.
    Scan {
        /// Repository root.
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// Number of worst modules to list.
        #[arg(long, default_value_t = 10)]
        top: usize,
        /// Exit 1 if Maintainability is below N.
        #[arg(long, value_name = "N")]
        fail_under: Option<f64>,
        /// Config file (default: `PATH/deadhand.toml` if present).
        #[arg(long)]
        config: Option<PathBuf>,
        /// Skip git history (Orphaned Code becomes unavailable, churn is unknown).
        #[arg(long)]
        no_git: bool,
    },
    /// Show every metric and piece of evidence for one module.
    Explain {
        file: PathBuf,
        /// Repository root.
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Compare the working tree against a git revision.
    Diff {
        #[arg(value_name = "GIT_REV")]
        revision: String,
        /// Repository root.
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// Exit 1 if Maintainability drops by more than N.
        #[arg(long, value_name = "N")]
        fail_on_drop: Option<f64>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Build a map of the repo: layers, directories, files and functions laid out in 2D.
    Map {
        /// Repository root.
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = MapFormat::Json)]
        format: MapFormat,
        /// Write to FILE instead of stdout.
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
        #[arg(long)]
        config: Option<PathBuf>,
        /// Skip git history (no churn or age on the map).
        #[arg(long)]
        no_git: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum MapFormat {
    /// The raw map model (schema in `deadhand_core::map`).
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Json,
}

/// Exit codes: 0 ok, 1 threshold failed, 2 usage/config error, 3 analysis error.
fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(err) => {
            eprintln!("deadhand: {err:#}");
            let usage = err.downcast_ref::<Error>().is_some_and(|e| matches!(e, Error::Config(_)))
                || err.downcast_ref::<UsageError>().is_some();
            ExitCode::from(if usage { 2 } else { 3 })
        }
    }
}

/// Bad arguments: exit code 2.
#[derive(Debug)]
struct UsageError(String);

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

fn load_config(root: &Path, path: Option<&Path>) -> Result<Config> {
    if !root.is_dir() {
        return Err(UsageError(format!("{} is not a directory", root.display())).into());
    }
    Ok(Config::load(root, path)?)
}

fn color() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

fn emit(text: &str) -> Result<()> {
    let mut out = std::io::stdout().lock();
    out.write_all(text.as_bytes())?;
    out.flush()?;
    Ok(())
}

/// Returns `Ok(false)` when a threshold failed.
fn run(cli: Cli) -> Result<bool> {
    match cli.command {
        Command::Scan { path, format, top, fail_under, config, no_git } => {
            let cfg = load_config(&path, config.as_deref())?;
            let started = Instant::now();
            let git: &dyn GitSource = if no_git { &NoGit } else { &GitCli };
            let report = deadhand_core::analyze_with(&path, &cfg, git)
                .with_context(|| format!("scanning {}", path.display()))?;
            let text = match format {
                Format::Json => render::json(&report)?,
                Format::Text => render::scan(&report, top, color(), Some(started.elapsed())),
            };
            emit(&text)?;
            Ok(fail_under.is_none_or(|n| report.maintainability >= n))
        }
        Command::Explain { file, root, format, config } => {
            let cfg = load_config(&root, config.as_deref())?;
            let canon_root = root.canonicalize().with_context(|| root.display().to_string())?;
            let canon_file = file.canonicalize().map_err(|e| UsageError(format!("{}: {e}", file.display())))?;
            let rel = canon_file
                .strip_prefix(&canon_root)
                .map_err(|_| UsageError(format!("{} is not inside {}", file.display(), root.display())))?;
            let rel = rel.to_string_lossy().replace('\\', "/");
            let report = deadhand_core::analyze(&root, &cfg)?;
            let module =
                report.module(&rel).ok_or_else(|| UsageError(format!("{rel} is not a scanned JS/TS module")))?;
            let text = match format {
                Format::Json => {
                    let ev: Vec<_> = report.evidence_for(&rel).collect();
                    let mut s = serde_json::to_string_pretty(
                        &serde_json::json!({ "schema_version": report.schema_version, "module": module, "evidence": ev }),
                    )?;
                    s.push('\n');
                    s
                }
                Format::Text => render::explain(&report, module, color()),
            };
            emit(&text)?;
            Ok(true)
        }
        Command::Diff { revision, root, format, fail_on_drop, config } => {
            let cfg = load_config(&root, config.as_deref())?;
            let diff = deadhand_core::diff::diff_against(&root, &revision, &cfg, &GitCli)?;
            let text = match format {
                Format::Json => {
                    let mut s = serde_json::to_string_pretty(&diff)?;
                    s.push('\n');
                    s
                }
                Format::Text => render::diff(&diff, color()),
            };
            emit(&text)?;
            Ok(fail_on_drop.is_none_or(|n| -diff.delta <= n))
        }
        Command::Map { path, format, output, config, no_git } => {
            let cfg = load_config(&path, config.as_deref())?;
            let git: &dyn GitSource = if no_git { &NoGit } else { &GitCli };
            let analysis = deadhand_core::analyze_full(&path, &cfg, git)
                .with_context(|| format!("scanning {}", path.display()))?;
            let map = deadhand_core::map::build(&analysis, &cfg);
            let text = match format {
                MapFormat::Json => render::map_json(&map)?,
            };
            match output {
                Some(file) => std::fs::write(&file, text).with_context(|| format!("writing {}", file.display()))?,
                None => emit(&text)?,
            }
            Ok(true)
        }
    }
}

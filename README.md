# Deadhand

[![CI](https://github.com/KrishnanKamatchi/deadhand/actions/workflows/ci.yml/badge.svg)](https://github.com/KrishnanKamatchi/deadhand/actions/workflows/ci.yml)

> Your AI wrote it. Deadhand checks if you can still own it.

Deadhand statically analyzes a JavaScript/TypeScript repository and measures how hard it would be
for a person to understand and safely change it, without AI assistance. It is deterministic,
offline, and makes no LLM calls. Every score traces back to concrete evidence: counts, paths and
line numbers.

It is not an AI-authorship detector and not a linter.

## Install

No Rust toolchain needed. Prebuilt binaries are published for Linux (x86_64, arm64, static musl),
macOS (Intel, Apple Silicon) and Windows (x86_64).

```sh
# macOS / Linux
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/KrishnanKamatchi/deadhand/releases/latest/download/deadhand-installer.sh | sh

# Windows (PowerShell)
powershell -ExecutionPolicy Bypass -c "irm https://github.com/KrishnanKamatchi/deadhand/releases/latest/download/deadhand-installer.ps1 | iex"
```

### Direct downloads

Each link always points to the newest release. Older versions are on the
[releases page](https://github.com/KrishnanKamatchi/deadhand/releases).

| Platform | Download | Checksum |
|---|---|---|
| macOS, Apple Silicon | [deadhand-aarch64-apple-darwin.tar.xz](https://github.com/KrishnanKamatchi/deadhand/releases/latest/download/deadhand-aarch64-apple-darwin.tar.xz) | [sha256](https://github.com/KrishnanKamatchi/deadhand/releases/latest/download/deadhand-aarch64-apple-darwin.tar.xz.sha256) |
| macOS, Intel | [deadhand-x86_64-apple-darwin.tar.xz](https://github.com/KrishnanKamatchi/deadhand/releases/latest/download/deadhand-x86_64-apple-darwin.tar.xz) | [sha256](https://github.com/KrishnanKamatchi/deadhand/releases/latest/download/deadhand-x86_64-apple-darwin.tar.xz.sha256) |
| Linux, x86_64 | [deadhand-x86_64-unknown-linux-gnu.tar.xz](https://github.com/KrishnanKamatchi/deadhand/releases/latest/download/deadhand-x86_64-unknown-linux-gnu.tar.xz) | [sha256](https://github.com/KrishnanKamatchi/deadhand/releases/latest/download/deadhand-x86_64-unknown-linux-gnu.tar.xz.sha256) |
| Linux, x86_64 (static, any distro) | [deadhand-x86_64-unknown-linux-musl.tar.xz](https://github.com/KrishnanKamatchi/deadhand/releases/latest/download/deadhand-x86_64-unknown-linux-musl.tar.xz) | [sha256](https://github.com/KrishnanKamatchi/deadhand/releases/latest/download/deadhand-x86_64-unknown-linux-musl.tar.xz.sha256) |
| Linux, arm64 | [deadhand-aarch64-unknown-linux-gnu.tar.xz](https://github.com/KrishnanKamatchi/deadhand/releases/latest/download/deadhand-aarch64-unknown-linux-gnu.tar.xz) | [sha256](https://github.com/KrishnanKamatchi/deadhand/releases/latest/download/deadhand-aarch64-unknown-linux-gnu.tar.xz.sha256) |
| Windows, x86_64 | [deadhand-x86_64-pc-windows-msvc.zip](https://github.com/KrishnanKamatchi/deadhand/releases/latest/download/deadhand-x86_64-pc-windows-msvc.zip) | [sha256](https://github.com/KrishnanKamatchi/deadhand/releases/latest/download/deadhand-x86_64-pc-windows-msvc.zip.sha256) |

To install from an archive, extract it and put the `deadhand` binary (`deadhand.exe` on Windows)
somewhere on your `PATH`:

```sh
tar -xJf deadhand-x86_64-unknown-linux-gnu.tar.xz
sudo mv deadhand-x86_64-unknown-linux-gnu/deadhand /usr/local/bin/
deadhand --version
```

On macOS, a binary downloaded through a browser may be blocked by Gatekeeper until you run
`xattr -d com.apple.quarantine /usr/local/bin/deadhand`. The curl installer is not affected.

From source (Rust 1.96+):

```sh
cargo install --git https://github.com/KrishnanKamatchi/deadhand deadhand
```

## Usage

```sh
deadhand scan [PATH]              # default PATH = .
    --format text|json            # default text
    --top N                       # worst modules to list (default 10)
    --fail-under N                # exit 1 if Maintainability < N
    --config PATH                 # default: PATH/deadhand.toml if present
    --no-git                      # skip git history

deadhand explain <FILE> [--root PATH]    # every metric and piece of evidence for one module
deadhand diff <GIT_REV> [--root PATH]    # working tree vs a revision (temporary git worktree)
    --fail-on-drop N              # exit 1 if Maintainability drops by more than N

deadhand map [PATH]               # 2D map of layers, directories, files and functions
    -o, --output FILE             # default: stdout
    --format json                 # the map model; an HTML viewer is planned (docs/map-plan.md)
    --config PATH, --no-git       # as for scan
```

Exit codes: `0` ok, `1` threshold failed, `2` usage/config error, `3` analysis error.

## What it measures

| Metric | Question | Built from |
|---|---|---|
| Cognitive Load | Can I follow the logic? | SonarSource cognitive complexity and cyclomatic complexity per function |
| Readability | Can I scan it? | function/file length vs repo median, nesting, parameter count, vague names, mixed naming |
| Entanglement | What is it tied to? | fan-in/out, instability, import cycles (Tarjan SCC), shared modules importing upper layers |
| Context Depth | How much must I read first? | transitive value-import closure, weighted by 1/distance, and max chain depth |
| Blast Radius | What breaks if I change it? | transitive dependents × churn × (1 − coverage) |
| Pattern Drift | Does it look like its peers? | robust z-scores against same-layer peers, layer-order violations |
| Orphaned Code | Has anyone come back to it? | single-commit files, bulk commits never edited, critical files untouched for 180 days |

Each metric scores 0–100 per module (100 = easy for a person). The score blends 60% absolute
thresholds with 40% repo-relative rank, so a repo that is bad everywhere cannot look fine. Repo
scores are LOC-weighted means of non-test modules. **Maintainability** is the weighted mean of the
seven. When a metric is unavailable (for example, no git history), its weight is redistributed and
the report says so.

## How it reads a repo

- Files: `.ts .tsx .js .jsx .mjs .cjs .mts .cts`. Respects `.gitignore`. Skips `node_modules`,
  `dist`, `build`, `.next`, `coverage`, `*.d.ts`, `*.min.js` and files with an `@generated` header.
- Tests (`*.test.*`, `*.spec.*`, and `__tests__/`, `__mocks__/`, `fixtures/`, `test/`, `tests/`,
  `e2e/` directories) are parsed and reported, but are left out of repo scores and of the import
  graph's edges.
- Imports: ESM, `export … from`, `import type`, `import()` and `require()` with string literals,
  and `import x = require()`. Resolution uses `oxc_resolver` with your `tsconfig.json` paths,
  index files and TS-style `.js` → `.ts` specifiers.
- Named functions get their own scores: declarations, methods, arrows assigned to a name, and
  components wrapped in `forwardRef`/`memo`. Anonymous callbacks count toward the enclosing
  function, one nesting level deeper, as in the SonarSource spec.
- Git: a single `git log --numstat` call. The 180-day churn window ends at the newest commit, not
  at the current time, so results stay stable. Shallow clones are treated as having no history.
- Not supported yet: `.vue`, `.svelte` and `.astro` files. Workspace packages imported by package
  name count as external.

## Configuration (`deadhand.toml`)

Every key is optional. See [`crates/deadhand-core/src/config.rs`](crates/deadhand-core/src/config.rs)
for all defaults and [`docs/calibration.md`](docs/calibration.md) for the reasoning behind them.

```toml
include = ["src/**"]
exclude = ["src/generated/**"]

[layers]
order = ["ui", "route", "service", "data", "infra"]   # allowed direction: left → right
[layers.paths]
service = ["services", "usecases", "domain"]

[weights]
cognitive_load = 0.20
entanglement = 0.20

[thresholds]
cognitive_complexity = 15
max_nesting = 4
bulk_commit_lines = 800

[scoring]
absolute_share = 0.6

[coverage]
lcov = "coverage/lcov.info"
```

## Layout

```
crates/deadhand-core   analysis library (discover, parse, graph, git, layers, metrics, scoring, diff, map)
crates/deadhand-cli    thin binary: arguments and rendering only
fixtures/              small repos that trigger specific findings (used by tests)
```

## Development

Requires Rust 1.96 or newer. CI runs the same checks as below on Linux, macOS and Windows:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Report output is covered by snapshot tests. After an intended change to the output, accept the
new snapshots with:

```sh
INSTA_UPDATE=always cargo test --workspace
```

## Releasing

Releases are built by [dist](https://github.com/axodotdev/cargo-dist) in
`.github/workflows/release.yml`. Bump `version` in `crates/deadhand-cli/Cargo.toml` and
`crates/deadhand-core/Cargo.toml`, merge to `main`, then push a matching tag:

```sh
git tag v0.2.0
git push origin v0.2.0
```

The workflow builds every target and creates the GitHub release with the archives, checksums and
installers. Do not create the release in the GitHub UI first: the workflow creates it, and fails
if a release for that tag already exists.

## License

MIT. See [LICENSE](LICENSE).

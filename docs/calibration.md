# Calibration

Defaults live in `crates/deadhand-core/src/config.rs`. This file records why each one has its
value and what was measured. Change a default only with a written justification here.

## Validation runs

Release build on an 18-core Linux laptop, run on 2026-09-28.

| Repo | Files (tests) | LOC | Git | Wall time | Maintainability |
|---|---|---|---|---|---|
| colinhacks/zod | 520 (202) | 83k | full history | 1.05 s | 60 |
| excalidraw/excalidraw | 686 (142) | 179k | shallow → skipped | 0.04 s | 41 |
| microsoft/vscode | 13,543 (4,130) | 3.43M | shallow → skipped | 0.87 s | 42 |

- The target is 100k LOC in under 5 s. vscode (34× larger) finishes in under 1 s. When
  full history is present, `git log --numstat` takes most of the time (zod: about 1 s of 1.05 s).
- `diff HEAD~50` on zod takes 2.1 s: two full scans plus worktree setup and teardown.

### Sanity check of worst-module lists

- **excalidraw**: `components/App.tsx` ranks worst (6). It is the well-known god component that
  every feature passes through. Next come `actions/actionProperties.tsx` and
  `element/src/binding.ts`, both large and heavily cross-imported. This matches the maintainers'
  reputation for these files.
- **zod**: `v4/classic/from-json-schema.ts` (deeply nested schema conversion),
  `v4/core/schemas.ts` (the core, depended on by most of the package) and `v3/types.ts` (the v3
  monolith). These are the files a newcomer would find hardest.
- **vscode**: chat renderers, inline-completion models and explorer views: large, stateful,
  heavily imported. None of the top entries are generated or vendored code.

## Decisions made while validating

1. **Shallow clones disable git metrics.** In a `--depth 1` clone every file appears to have one
   commit, so Orphaned Code would flag the entire repo. Deadhand runs
   `git rev-parse --is-shallow-repository` and treats a shallow clone as having no history.
2. **Fixture and mock directories are tests.** In vscode, tagging `fixtures/`, `test/`, `tests/`,
   `e2e/` and `__mocks__/` as tests moved about 1,200 files (2,928 → 4,130 tests), including 27
   that are broken on purpose. As production code they distorted drift and filled the headline
   with parse errors.
3. **Top-level `return` is allowed.** CommonJS scripts (vscode `scripts/xterm-update.js`) may
   return at module scope. The parser now accepts this.
4. **Compact JSON.** For vscode, pretty-printed JSON is about 173 MB and compact JSON 98 MB (43%
   smaller), so `--format json` writes compact JSON. Pipe it to `jq` to read. Most of the size is
   per-module imports and function facts; a summary-only JSON mode is a candidate follow-up.

## Thresholds and why

| Key | Default | Reasoning |
|---|---|---|
| `cognitive_complexity` | 15 | SonarSource's default for "too complex" functions. |
| `cognitive_density_good` / `_bad` | 5 / 30 per 100 LOC | Clean fixtures sit at 0–5; the spaghetti route handler reaches 96. Measured median / p90 (non-test modules): zod 10 / 37, excalidraw 8.5 / 23, vscode 12 / 31. So 30 is roughly the p90 of real code: the densest tenth scores zero on this signal. |
| p90 baseline | used only when p90 ≥ threshold/2 | Pure p90 would always flag 10% of functions, including trivial ones (p90 = 5 in zod v4 core). |
| `max_nesting` | 4 | Four levels is where a reader has to keep a stack in their head. |
| `max_params` | 5 | Beyond five positional arguments, call sites stop being readable without the signature open. |
| `readability_good` / `_bad` | 0 / 8 points | One point per clear problem, so 8 problems in one file scores zero. |
| `fan_out_good` / `_bad` | 5 / 20 | Measured median / p90 internal fan-out: zod 1 / 3, excalidraw 3 / 12, vscode 6 / 24. 5 keeps typical modules at full marks; 20 is around vscode's p90, where a file touches most of a feature. |
| `context_good` / `_bad` | 5 / 40 (Σ 1/d) | 5 is about five direct dependencies. 40 is only reached by modules at the centre of large cycles. |
| `context_depth_good` / `_bad` | 4 / 12 | Added because Σ 1/d scores a 12-long chain as 3.1 (cheap), yet a reader must still follow all 12 hops. The `deep-context` fixture's entry point sits at depth 12. |
| `blast_good` / `_bad` | 5 / 60 | Dependents × churn factor (0.5–2.0). A stable module with 60 dependents, or a hot one with 30, scores zero. |
| `drift_good` / `_bad` | 0.5 / 3.0 | Drift is Σ min(\|z\|, 6) over outlying features ÷ 8. One strongly deviating feature is about 0.75; the `drift` fixture's outlier deviates on 6 features and reaches about 3.8. |
| Drift z-scale floors | fan-out 2, exports 2, layers 1, LOC 20, function LOC 5, nesting 1, packages 2 | Without floors, peers of near-identical size made a 6-line file look like an extreme outlier against 1-line peers (MAD = 0). |
| `bulk_commit_lines` | 800 | Larger than almost any hand-written change; typical of scaffolding, vendored or pasted code. |
| `critical_fan_in` | 5 | A module five others depend on is load-bearing. |
| Orphan bulk flag | only if never edited since | A later edit is evidence someone engaged with the code, which is exactly what this metric asks. |
| `absolute_share` | 0.6 | Absolute thresholds dominate so a uniformly poor repo cannot look fine. A module with a perfect absolute score also gets a perfect relative one, so a uniformly clean repo is not penalised by rank alone. |

## Known limitations

- Vague-name detection flags `data` and single-letter names, which is noisy in React code
  (`const { data } = useQuery()`). Arrow-function parameters are exempt.
- Cycles are computed over value imports only. Type-only cycles are not runtime cycles.
- Layer detection is path based. Repos that organise by feature (`orders/`, `users/`) mostly land
  in `unknown`, so layer metrics are quiet there.

# Plan: `deadhand map`, a code map you can explore

Status: proposal, not built yet.

## 1. Goal

Deadhand's text report says which files are hard to own. It does not show how the codebase is
shaped. `deadhand map` will turn a scan into one large map you can pan and zoom. The aim is
that after ten minutes of looking around you have a mental model of the repo: where things live,
how they depend on each other, where the trouble is, and what breaks if you touch something.

This is not an AST viewer and not a generic force-directed "hairball" graph. Every pixel should
answer one of these questions:

| Question | How the map answers it |
|---|---|
| Where does this live? | Directories are **districts**, files are **buildings**, and positions stay the same between scans |
| How is it organised? | Architectural layers are horizontal **bands**: UI at the top, infra at the bottom |
| Is it healthy? | A colour **lens**: Maintainability or any one of the seven metrics |
| What does it depend on? | **Context trail**: highlights the transitive imports you must read first |
| What breaks if I change it? | **Blast ripple**: dependents light up in rings by distance |
| Where are the problems? | **Issue pins** from evidence, plus red cycle loops and upward (wrong-way) arrows |
| Is anyone maintaining it? | **Age/churn lens**: hotspots glow, orphaned files fade out |

The map keeps Deadhand's existing rules: deterministic, offline, no LLM calls, and every visual
traces back to evidence with paths and line numbers.

## 2. What it looks like

```
┌────────────────────────────────────────────────────────────────────────────┐
│ deadhand map · my-app · Maintainability 58    [Lens: Maintainability ▾] 🔍 │
├────────────────────────────────────────────────────────────────┬───────────┤
│ UI ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │ App.tsx   │
│  ┌ components ────────────┐ ┌ pages ───────────┐               │ ui · 412  │
│  │ ▇App ⚑3  ▆Nav  ▂Btn   │ │ ▅Home  ▇Checkout │               │ M: 31 ▇▇▁ │
│  └────────────────────────┘ └──────────┬───────┘               │           │
│ ROUTE ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│░░░░░░░░░░░░░░░░░░░░░░░ │ Weakest:  │
│  ┌ routes ───────┐                     │                       │ Cognitive │
│  │ ▆orders ⚑1    │                     ▼                       │ Blast     │
│ SERVICE ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │           │
│  ┌ services ─────────────┐   ↺ cycle: billing ⇄ orders         │ ⚑ render()│
│  │ ▇billing ⚑2  ▅orders  │                                     │  cog 41   │
│ DATA ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │  L88–240  │
│  ┌ db ──┐ ⇡ db/client imports ui/format (layer violation)       │           │
│  │ ▃client│                                                      │ [Ripple]  │
│  └──────┘                                         ┌─minimap─┐   │ [Trail]   │
│                                                   │ ▪▪ ▫ ▪  │   │           │
└───────────────────────────────────────────────────┴─────────┴───┴───────────┘
  ▇ = colour by lens · block area = LOC · ⚑ = evidence pins · ↺ cycle · ⇡ upward edge
```

### Semantic zoom (detail appears as you zoom in)

| Zoom | You see | Labels |
|---|---|---|
| **Z0 Continent** | Layer bands, top-level districts as coloured masses, district-to-district flows (bundled, width = edge count), red cycle loops | Layer and district names, district score |
| **Z1 District** | Nested sub-districts and file buildings. Area = LOC, colour = lens, ⚑ pins for High/Warn evidence | File names, score badge |
| **Z2 Building** | A file split into **rooms** (its named functions), each sized by LOC and coloured by cognitive complexity, plus import/export "doors" on its edges | Function names, cog/cyclomatic/nesting |
| **Z3 Room** | One function: span, metrics, evidence messages, and (optional) the source with the hot lines marked | Full details in the side panel |

Edges are never all drawn at once. At Z0 they are aggregated per district pair. At Z1 and
deeper you see only the edges of the hovered or selected building, plus the always-on problem
edges (cycles, layer violations).

## 3. Interaction

- **Moving around**: drag or arrow/WASD keys to pan; wheel, pinch or `+`/`-` to zoom; double-click
  zooms to fit an item; `F` fits the whole repo. A minimap shows where you are, and you can drag
  on it. Breadcrumbs (`repo › src › services › billing.ts › charge()`) jump back up.
- **Search** (`/` or `Ctrl+K`): fuzzy search over files, directories, functions and evidence.
  Enter flies the camera to the result.
- **Lenses** (`1`–`9`): Maintainability, the seven metrics, Churn (recent commits),
  Age (days since last commit), Authors, Coverage (when lcov is configured), Tests (has a test
  file or not).
- **Selection modes** on a file:
  - *Ripple* (blast radius): dependents light up in rings 1, 2, 3+ hops away, everything else
    dims. Shows "N modules across M layers would be affected".
  - *Trail* (context depth): the import closure you must understand first, drawn as a path with
    hop numbers.
  - *Neighbours*: direct imports (out) and importers (in) only.
- **Issues panel**: the findings and evidence list, filtered by metric and severity. Clicking a
  row flies to the pin. **Tours** step through "10 worst modules", "every cycle" or "every layer
  violation" with Next/Prev.
- **Shareable views**: the camera, lens and selection go into the URL hash
  (`#z=2&at=src/services/billing.ts&lens=cognitive_load`), so a link opens the same view.
- **Filters**: hide tests, hide type-only edges, show only one layer or directory, and a
  "score below N" slider that dims healthy code.

## 4. Key design decisions

1. **Output is one self-contained HTML file** (`deadhand-map.html`). It opens offline in any
   browser with no server, no CDN and no network calls, and it can be attached to a PR or CI
   artifact. The viewer JS and CSS are embedded in the binary with `include_str!`.
2. **Layout is computed in Rust, not in the browser.** It is deterministic and covered by snapshot
   tests, and it is **stable**: a file keeps its place between scans unless its directory
   changes. Stable positions are what let spatial memory form. Force-directed layouts reshuffle on
   every run, so they destroy that memory.
3. **Layout algorithm**:
   - Vertical axis = **layer** (from `[layers]` config or detection; unknown is a band at the
     bottom). This makes wrong-way dependencies visible as arrows that point up.
   - Inside a band, directories are nested rectangles from an **ordered squarified treemap**,
     sorted by path so that adding a file only nudges its neighbours. Area = LOC, with a minimum
     size so tiny files stay clickable.
   - A directory that spans several layers is split into one sub-district per band, and the parts
     are tied together with a faint outline.
   - Rooms (functions) inside a building use the same treemap, ordered by source line.
4. **2D, rendered on a canvas** (Canvas2D, culled with a quadtree, with level-of-detail), not 3D.
   A 3D "code city" looks impressive, but buildings hide each other and labels are hard to read.
   A height cue can be added later as a shadow or bar. The renderer has to stay smooth on
   vscode-sized repos (about 13.5k files and 100k functions).
5. **No JS toolchain in the build.** The viewer is plain ES2020 modules concatenated at build
   time, so CI stays `cargo fmt/clippy/test/doc` with no Node needed to build.
6. **Source code is not embedded by default** (output size and privacy). `--with-source` adds it
   for the Z3 code view.

## 5. Architecture changes

### deadhand-core

1. **Refactor `analyze_with`** so it returns an internal `Analysis { parsed, graph, layers,
   git_facts, outputs, scores }` that `build_report` and the new map builder both read. The JSON
   report does not change.
2. **New module `map/`**:
   - `map/model.rs`: the serializable `MapModel` (its own `MAP_SCHEMA_VERSION`):
     - `districts`: tree of `{ id, path, layer, rect, loc, score, children }`
     - `buildings`: `{ id, path, district, rect, loc, is_test, layer, scores, raw, git: {commits,
       recent, authors, last_commit_days}, coverage, symbols, exports }`
     - `rooms`: `{ building, name, span, rect, loc, cognitive, cyclomatic, nesting, params }`
     - `edges`: `{ from, to, type_only, kinds, in_cycle, layer_violation }`
     - `flows`: aggregated district-to-district edges
     - `pins`: evidence mapped to building/room ids with severity, metric, message, span
     - `cycles`, `findings`, `summary`, `metrics` (reused from `RepoReport`)
     - `closures`: not precomputed (too large); the viewer computes ripple and trail with BFS over
       `edges`, which is fast in JS.
   - `map/layout.rs`: bands, ordered squarified treemap, minimum-size handling, room layout.
     Pure functions with unit tests (no overlaps, children inside parents, deterministic output).
   - `map/bundle.rs`: aggregates edges into district flows, and maps evidence spans to rooms.
3. **Small extra facts** (cheap, already almost collected):
   - `days since last commit` per file (from `FileHistory.last_commit` and `head_time`).
   - Optionally, phase 5: **call edges** inside a file (the visitor records calls to named local
     functions) so rooms can show "who calls whom" inside a building.

### deadhand-cli

```
deadhand map [PATH]
    -o, --output FILE      # default: deadhand-map.html
    --format html|json     # json = the raw MapModel, for other tools and for tests
    --open                 # open the result in the default browser
    --with-source          # embed source for the Z3 code view
    --config PATH, --no-git   # same as scan
```

- `render/map.rs`: injects `MapModel` JSON into the HTML template as
  `<script type="application/json">`, with `<`, `>` and `&` escaped so no `</script>` breakout is
  possible.
- `viewer/`: `index.html`, `style.css`, and JS modules: `camera.js` (pan/zoom/fly-to),
  `render.js` (canvas, culling, LOD), `lenses.js`, `graph.js` (ripple/trail BFS), `ui.js`
  (side panel, search, issues, tours, minimap), `url.js` (hash state).

## 6. Phases

| Phase | Delivers | Done when |
|---|---|---|
| **1. Model + layout** | `Analysis` refactor, `map/` module, `deadhand map --format json` | Fixture snapshots stable; layout invariant tests pass; `scan` output byte-identical |
| **2. Viewer MVP** | HTML output: bands, districts, buildings, pan/zoom, minimap, lens switch, side panel, search | `spaghetti` and `cycles` fixtures read clearly; zod scan smooth at 60 fps |
| **3. Relationships** | District flows, selection edges, Ripple, Trail, cycle loops, upward violation arrows, filters | You can answer "what breaks if I edit X" from the map alone |
| **4. Issues** | Evidence pins, issues panel, tours, URL deep links | Every finding in `scan` can be reached in one click |
| **5. Inside files** | Rooms (Z2), function details (Z3), `--with-source`, optional intra-file call edges | Zooming into `excalidraw/App.tsx` shows its hot functions |
| **6. Change view** (later) | `deadhand map --against REV`: colour by score delta, new/removed buildings outlined | PR reviewers can see what got worse |

## 7. Testing

- **Core**: `insta` JSON snapshots of `MapModel` for every fixture; property tests for layout (no
  sibling overlap, children contained, area roughly proportional to LOC, same input gives the
  same output); a stability test (add one file and check that unrelated buildings move less
  than a threshold).
- **CLI**: snapshot of the HTML head and data block; an escaping test with a file named
  `</script>.ts`.
- **Viewer**: an optional Playwright smoke test (load the fixture map, zoom, search, open a
  ripple, take a screenshot). It runs locally and is not required in CI, to keep CI free of Node.
- **Performance**: record map build time and HTML size for zod, excalidraw and vscode in
  `docs/calibration.md`. Target: under 1 s extra on top of the scan, and under about 15 MB of
  HTML for vscode without source.

## 8. Risks

| Risk | Mitigation |
|---|---|
| Huge repos make a huge HTML file | Compact JSON (short keys, string table for paths); rooms included only when below a size limit, or with `--rooms` |
| Treemap slivers are unreadable | Minimum area, aspect-ratio-aware squarify, and directories with a single child are collapsed (`src/a/b` becomes one district) |
| Layer config missing, so everything is in one band | Fall back to path-based detection (already present); one band still works as a plain directory map |
| Edge clutter | Edges are aggregated by default and shown in detail only on selection; problem edges are always shown |

## 9. Open questions

1. Is a self-contained HTML file fine, or should there also be `deadhand map --serve`, a local
   server that re-scans on file change?
2. 2D map (recommended) or a 3D code city?
3. Should `--with-source` exist at all, or should the map stay metrics-only?

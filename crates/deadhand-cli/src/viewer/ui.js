const panel = document.getElementById("panel");
const tooltip = document.getElementById("tooltip");
const searchInput = document.getElementById("search");
const resultsList = document.getElementById("results");
const lensSelect = document.getElementById("lens");

function el(tag, props, ...kids) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(props || {})) {
    if (k === "class") node.className = v;
    else if (k.startsWith("on")) node.addEventListener(k.slice(2), v);
    else node.setAttribute(k, v);
  }
  for (const kid of kids.flat(Infinity)) {
    if (kid === null || kid === undefined || kid === false) continue;
    node.append(kid instanceof Node ? kid : String(kid));
  }
  return node;
}


function hitTest(px, py) {
  const [x, y] = toWorld(px, py);
  const inside = (r) => x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h;
  for (let i = 0; i < buildings.length; i++) {
    if (inside(buildings[i].rect)) return { kind: "building", index: i };
  }
  let best = null;
  districts.forEach((d, i) => {
    if (inside(d.rect) && (best === null || d.depth >= districts[best].depth)) best = i;
  });
  return best === null ? null : { kind: "district", index: best };
}

function rectOf(item) {
  return item.kind === "building" ? buildings[item.index].rect : districts[item.index].rect;
}

function select(item, fly) {
  state.selected = item;
  renderPanel();
  if (item && fly) flyTo(framing(rectOf(item), item.kind === "building" ? 0.3 : 0.85));
  requestDraw();
}


function crumbs(item) {
  const band = item.kind === "building" ? districts[buildings[item.index].district].band : districts[item.index].band;
  const trail = districtTrail(item.kind === "building" ? buildings[item.index].district : item.index);
  const parts = [el("span", null, bandName(band))];
  for (const d of trail) {
    if (item.kind === "district" && d === item.index) break;
    parts.push(" / ", el("button", { onclick: () => select({ kind: "district", index: d }, true) }, districtLabel(d)));
  }
  return el("div", { class: "crumbs" }, parts);
}

function scoreRows(entries) {
  return el(
    "div",
    { class: "rows" },
    entries.map(([label, score]) => [
      el("span", null, label),
      el("div", { class: "meter" }, el("div", { style: `width:${Math.max(0, Math.min(100, score ?? 0))}%;background:${scoreColor(score)}` })),
      el("span", { class: "val" }, round(score)),
    ]),
  );
}

function big(score, label) {
  return el(
    "div",
    { class: "big" },
    el("span", { class: "chip", style: `background:${scoreColor(score)}` }),
    el("span", { class: "num" }, round(score)),
    el("span", { class: "lbl" }, label),
  );
}

function fileLink(i) {
  return el("button", { class: "link grow", title: buildings[i].path, onclick: () => select({ kind: "building", index: i }, true) }, buildings[i].path);
}

function edgeTags(e) {
  const tags = [];
  if (e.type_only) tags.push("type");
  if (e.in_cycle) tags.push("cycle");
  if (e.violation === "inverted") tags.push("upward");
  if (e.violation === "skips_service") tags.push("skips service");
  return tags.length ? el("span", { class: "num" }, tags.join(", ")) : null;
}

function evidenceList(pins) {
  const order = { high: 0, warn: 1, info: 2 };
  const sorted = [...pins].sort((a, b) => order[a.severity] - order[b.severity]);
  return el(
    "ul",
    { class: "list" },
    sorted.map((p) =>
      el(
        "li",
        null,
        el("span", { class: `sev ${p.severity}` }, p.severity),
        el("span", { class: "msg" }, p.message, p.span ? el("span", { class: "num" }, `  line ${p.span.start_line}`) : null),
      ),
    ),
  );
}

function buildingPanel(i) {
  const b = buildings[i];
  const tags = [b.layer ? `${b.layer} layer` : "unknown layer", `${b.loc} lines`, `${b.room_count} functions`];
  if (b.is_test) tags.push("test file");
  if (b.external_packages.length) tags.push(`${b.external_packages.length} packages`);
  const metrics = MAP.metrics.filter((m) => b.scores[m.kind] !== undefined).map((m) => [m.label, b.scores[m.kind]]);
  const fns = rooms
    .slice(b.first_room, b.first_room + b.room_count)
    .sort((a, c) => c.cognitive - a.cognitive || a.span.start_line - c.span.start_line)
    .slice(0, 12);
  const imports = importsOf[i];
  const importers = importersOf[i];
  const history = [];
  if (b.git) {
    history.push(["Commits", b.git.commits], ["Recent commits", b.git.recent_commits], ["Authors", b.git.authors]);
    history.push(["Last change", `${b.git.days_since_last_commit} days before newest commit`]);
  }
  if (b.coverage !== undefined) history.push(["Line coverage", `${Math.round(b.coverage * 100)}%`]);

  return [
    crumbs({ kind: "building", index: i }),
    el("h2", null, b.label),
    el("div", { class: "tags" }, tags.map((t) => el("span", { class: "tag" }, t))),
    big(b.maintainability, "Maintainability"),
    el("h3", null, "Scores"),
    scoreRows(metrics),
    pinsOf[i].length ? [el("h3", null, `Evidence (${pinsOf[i].length})`), evidenceList(pinsOf[i])] : null,
    fns.length
      ? [
          el("h3", null, "Most complex functions"),
          el(
            "ul",
            { class: "list" },
            fns.map((f) =>
              el(
                "li",
                null,
                el("span", { class: "grow", title: f.name }, f.name),
                el("span", { class: "num" }, `line ${f.span.start_line} · cog ${f.cognitive} · nest ${f.max_nesting}`),
              ),
            ),
          ),
        ]
      : null,
    history.length ? [el("h3", null, "History"), el("dl", { class: "kv" }, history.map(([k, v]) => [el("dt", null, k), el("dd", null, v)]))] : null,
    el("h3", null, `Imports (${imports.length})`),
    imports.length ? el("ul", { class: "list" }, imports.map((e) => el("li", null, fileLink(e.to), edgeTags(e)))) : el("p", { class: "note" }, "No internal imports."),
    el("h3", null, `Imported by (${importers.length})`),
    importers.length
      ? el("ul", { class: "list" }, importers.map((e) => el("li", null, fileLink(e.from), edgeTags(e))))
      : el("p", { class: "note" }, b.is_test ? "Test files are left out of the import graph." : "Nothing imports this file."),
  ];
}

function filesUnder(d) {
  const out = [...districtBuildings[d]];
  for (const c of districtChildren[d]) out.push(...filesUnder(c));
  return out;
}

function districtPanel(i) {
  const d = districts[i];
  const files = filesUnder(i)
    .filter((b) => !buildings[b].is_test)
    .sort((a, b) => buildings[a].maintainability - buildings[b].maintainability)
    .slice(0, 10);
  return [
    crumbs({ kind: "district", index: i }),
    el("h2", null, d.path || MAP.root_name),
    el("div", { class: "tags" }, [`${bandName(d.band)} layer`, `${d.files} files`, `${d.loc} lines`].map((t) => el("span", { class: "tag" }, t))),
    big(d.maintainability, "Maintainability (by lines of code)"),
    files.length ? [el("h3", null, "Lowest scoring files"), el("ul", { class: "list" }, files.map((b) => el("li", null, fileLink(b), el("span", { class: "num" }, round(buildings[b].maintainability)))))] : null,
    districtChildren[i].length
      ? [
          el("h3", null, "Folders"),
          el(
            "ul",
            { class: "list" },
            districtChildren[i].map((c) =>
              el(
                "li",
                null,
                el("button", { class: "link grow", onclick: () => select({ kind: "district", index: c }, true) }, districtLabel(c)),
                el("span", { class: "num" }, round(districts[c].maintainability)),
              ),
            ),
          ),
        ]
      : null,
  ];
}

function repoPanel() {
  const s = MAP.summary;
  const metrics = MAP.metrics.filter((m) => m.available).map((m) => [m.label, m.score]);
  const sevClass = { high: "high", warn: "warn", info: "info" };
  return [
    el("h2", null, MAP.root_name),
    el("div", { class: "tags" }, [`${s.files} files`, `${s.test_files} tests`, `${s.loc} lines`, `${s.functions} functions`].map((t) => el("span", { class: "tag" }, t))),
    big(MAP.maintainability, "Maintainability"),
    el("h3", null, "Scores"),
    scoreRows(metrics),
    MAP.findings.length
      ? [
          el("h3", null, "Findings"),
          el(
            "ul",
            { class: "list" },
            MAP.findings.map((f) => el("li", null, el("span", { class: `sev ${sevClass[f.severity]}` }, f.severity), el("span", { class: "msg" }, f.message))),
          ),
        ]
      : null,
    el("h3", null, "Reading the map"),
    el(
      "p",
      { class: "note" },
      "Each horizontal band is an architectural layer, top to bottom in dependency order. Boxes inside a band are folders, coloured blocks are files sized by lines of code. Click a file or folder for details.",
    ),
    [...MAP.notes, ...MAP.metrics.flatMap((m) => m.notes)].map((n) => el("p", { class: "note" }, n)),
  ];
}

function renderPanel() {
  const sel = state.selected;
  const content = !sel ? repoPanel() : sel.kind === "building" ? buildingPanel(sel.index) : districtPanel(sel.index);
  panel.replaceChildren(...content.flat(2).filter(Boolean));
  panel.scrollTop = 0;
}


let searchIndex = null;
let results = [];
let active = 0;

function buildSearchIndex() {
  const entries = [];
  buildings.forEach((b, i) => entries.push({ kind: "file", text: b.path, item: { kind: "building", index: i } }));
  districts.forEach((d, i) => {
    if (d.path) entries.push({ kind: "folder", text: `${d.path}  (${bandName(d.band)})`, key: d.path, item: { kind: "district", index: i } });
  });
  rooms.forEach((r) =>
    entries.push({ kind: "function", text: `${r.name}  ${buildings[r.building].path}`, key: r.name, item: { kind: "building", index: r.building } }),
  );
  return entries;
}

function matchScore(text, q) {
  const t = text.toLowerCase();
  const at = t.indexOf(q);
  if (at >= 0) {
    const base = t.lastIndexOf("/", at) + 1;
    return 1000 - at * 0.5 - t.length * 0.01 + (at === base ? 300 : 0);
  }
  let j = 0;
  let gaps = 0;
  let last = -1;
  for (let k = 0; k < t.length && j < q.length; k++) {
    if (t[k] === q[j]) {
      if (last >= 0) gaps += k - last - 1;
      last = k;
      j++;
    }
  }
  return j === q.length ? 200 - gaps - t.length * 0.01 : -1;
}

function runSearch() {
  const q = searchInput.value.trim().toLowerCase();
  state.matches = new Set();
  if (!q) {
    resultsList.hidden = true;
    results = [];
    requestDraw();
    return;
  }
  searchIndex = searchIndex || buildSearchIndex();
  const scored = [];
  for (const e of searchIndex) {
    const s = matchScore(e.key || e.text, q);
    if (s >= 0) scored.push([s, e]);
  }
  scored.sort((a, b) => b[0] - a[0] || a[1].text.localeCompare(b[1].text));
  results = scored.slice(0, 40).map(([, e]) => e);
  for (const e of results) if (e.item.kind === "building") state.matches.add(e.item.index);
  active = 0;
  renderResults();
  requestDraw();
}

function renderResults() {
  resultsList.replaceChildren(
    ...results.map((e, k) =>
      el(
        "li",
        { "aria-selected": String(k === active), onmousedown: (ev) => (ev.preventDefault(), choose(k)) },
        el("span", { class: "kind" }, e.kind),
        el("span", { class: "text", title: e.text }, e.text),
      ),
    ),
  );
  if (!results.length) resultsList.replaceChildren(el("li", null, el("span", { class: "text" }, "No matches")));
  resultsList.hidden = false;
}

function choose(k) {
  const e = results[k];
  if (!e) return;
  resultsList.hidden = true;
  searchInput.blur();
  select(e.item, true);
}

searchInput.addEventListener("input", runSearch);
searchInput.addEventListener("focus", () => searchInput.value && runSearch());
searchInput.addEventListener("blur", () => (resultsList.hidden = true));
searchInput.addEventListener("keydown", (ev) => {
  if (ev.key === "ArrowDown" || ev.key === "ArrowUp") {
    ev.preventDefault();
    active = clamp(active + (ev.key === "ArrowDown" ? 1 : -1), 0, Math.max(0, results.length - 1));
    renderResults();
  } else if (ev.key === "Enter") {
    choose(active);
  } else if (ev.key === "Escape") {
    searchInput.value = "";
    runSearch();
    searchInput.blur();
  }
});


const pointers = new Map();
let drag = null;

// Capture can fail if the pointer is already gone (released, or a synthetic event); input still works.
function capture(target, id) {
  try {
    target.setPointerCapture(id);
  } catch {
    return;
  }
}

function showTooltip(hit, px, py) {
  if (!hit) {
    tooltip.hidden = true;
    return;
  }
  const lens = state.lens;
  if (hit.kind === "building") {
    const b = buildings[hit.index];
    tooltip.replaceChildren(
      el("div", { class: "path" }, b.path),
      el("div", { class: "meta" }, `${lens.label} ${round(lens.value(b))} · ${b.loc} lines${b.is_test ? " · test" : ""}`),
    );
  } else {
    const d = districts[hit.index];
    tooltip.replaceChildren(
      el("div", { class: "path" }, d.path || MAP.root_name),
      el("div", { class: "meta" }, `Maintainability ${round(d.maintainability)} · ${d.files} files · ${d.loc} lines`),
    );
  }
  tooltip.hidden = false;
  const box = tooltip.getBoundingClientRect();
  tooltip.style.left = `${Math.min(px + 14, view.w - box.width - 8)}px`;
  tooltip.style.top = `${Math.min(py + 14, view.h - box.height - 8)}px`;
}

canvas.addEventListener("pointerdown", (ev) => {
  capture(canvas, ev.pointerId);
  pointers.set(ev.pointerId, [ev.offsetX, ev.offsetY]);
  if (pointers.size === 1) drag = { x: ev.offsetX, y: ev.offsetY, moved: false };
});

canvas.addEventListener("pointermove", (ev) => {
  const prev = pointers.get(ev.pointerId);
  if (prev) pointers.set(ev.pointerId, [ev.offsetX, ev.offsetY]);
  if (pointers.size === 2 && prev) {
    const [a, b] = [...pointers.values()];
    const other = a === pointers.get(ev.pointerId) ? b : a;
    const before = Math.hypot(prev[0] - other[0], prev[1] - other[1]);
    const after = Math.hypot(ev.offsetX - other[0], ev.offsetY - other[1]);
    if (before > 0) zoomAt((ev.offsetX + other[0]) / 2, (ev.offsetY + other[1]) / 2, after / before);
    if (drag) drag.moved = true;
    return;
  }
  if (drag && prev) {
    const dx = ev.offsetX - prev[0];
    const dy = ev.offsetY - prev[1];
    if (!drag.moved && Math.hypot(ev.offsetX - drag.x, ev.offsetY - drag.y) > 4) {
      drag.moved = true;
      canvas.classList.add("dragging");
      tooltip.hidden = true;
    }
    if (drag.moved) panBy(dx, dy);
    return;
  }
  const hit = hitTest(ev.offsetX, ev.offsetY);
  const same = hit && state.hover && hit.kind === state.hover.kind && hit.index === state.hover.index;
  if (!same) {
    state.hover = hit;
    requestDraw();
  }
  showTooltip(hit, ev.offsetX, ev.offsetY);
});

function endPointer(ev) {
  const wasClick = drag && !drag.moved && pointers.size === 1;
  pointers.delete(ev.pointerId);
  if (wasClick && ev.type === "pointerup") select(hitTest(ev.offsetX, ev.offsetY), false);
  if (pointers.size === 0) {
    drag = null;
    canvas.classList.remove("dragging");
  }
}
canvas.addEventListener("pointerup", endPointer);
canvas.addEventListener("pointercancel", endPointer);
canvas.addEventListener("pointerleave", () => {
  if (pointers.size) return;
  state.hover = null;
  tooltip.hidden = true;
  requestDraw();
});

canvas.addEventListener(
  "wheel",
  (ev) => {
    ev.preventDefault();
    const unit = ev.deltaMode === 1 ? 0.05 : ev.deltaMode === 2 ? 1 : 0.0015;
    zoomAt(ev.offsetX, ev.offsetY, Math.exp(-ev.deltaY * unit * (ev.ctrlKey ? 4 : 1)));
  },
  { passive: false },
);

canvas.addEventListener("dblclick", (ev) => {
  const hit = hitTest(ev.offsetX, ev.offsetY);
  if (hit) select(hit, true);
});

function centreOnMinimap(ev) {
  const x = ev.offsetX / miniView.s;
  const y = ev.offsetY / miniView.s;
  jumpTo({ x: x - view.w / 2 / cam.s, y: y - view.h / 2 / cam.s, s: cam.s });
}
mini.addEventListener("pointerdown", (ev) => {
  capture(mini, ev.pointerId);
  centreOnMinimap(ev);
});
mini.addEventListener("pointermove", (ev) => {
  if (ev.buttons & 1) centreOnMinimap(ev);
});


window.addEventListener("keydown", (ev) => {
  const typing = ev.target === searchInput || ev.target === lensSelect;
  if ((ev.key === "k" && (ev.ctrlKey || ev.metaKey)) || (ev.key === "/" && !typing)) {
    ev.preventDefault();
    searchInput.focus();
    searchInput.select();
    return;
  }
  if (typing || ev.ctrlKey || ev.metaKey || ev.altKey) return;
  const step = 90;
  switch (ev.key) {
    case "f":
    case "F":
      fitAll();
      break;
    case "+":
    case "=":
      zoomAt(view.w / 2, view.h / 2, 1.4);
      break;
    case "-":
    case "_":
      zoomAt(view.w / 2, view.h / 2, 1 / 1.4);
      break;
    case "ArrowLeft":
      panBy(step, 0);
      break;
    case "ArrowRight":
      panBy(-step, 0);
      break;
    case "ArrowUp":
      panBy(0, step);
      break;
    case "ArrowDown":
      panBy(0, -step);
      break;
    case "Escape":
      state.matches = new Set();
      select(null, false);
      break;
    default: {
      const n = Number(ev.key);
      if (Number.isInteger(n) && n >= 1 && n <= LENSES.length) setLens(n - 1);
      return;
    }
  }
  ev.preventDefault();
});


function setLens(k) {
  state.lens = LENSES[k];
  lensSelect.value = String(k);
  recolor();
}

lensSelect.replaceChildren(...LENSES.map((l, k) => el("option", { value: String(k) }, `${k + 1}  ${l.label}`)));
lensSelect.addEventListener("change", () => setLens(Number(lensSelect.value)));

document.title = `deadhand map - ${MAP.root_name}`;
document.getElementById("repo").textContent = MAP.root_name;
document.getElementById("headline").textContent =
  `Maintainability ${round(MAP.maintainability)} · ${MAP.summary.files} files · ${MAP.summary.loc} lines`;
document.querySelector(".ramp").style.background =
  `linear-gradient(to right, ${RAMP.map(([s, c]) => `rgb(${c.join(",")}) ${s}%`).join(", ")})`;

readTheme();
resize();
setLimits();
jumpTo(framing(MAP.bounds, 0.94));
recolor();
renderPanel();
if (!buildings.length) document.getElementById("empty").hidden = false;

window.addEventListener("resize", () => {
  resize();
  setLimits();
});
window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
  readTheme();
  recolor();
});

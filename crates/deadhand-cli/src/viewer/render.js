const canvas = document.getElementById("map");
const ctx = canvas.getContext("2d");
const mini = document.getElementById("mini");
const miniCtx = mini.getContext("2d");
const miniBase = document.createElement("canvas");
const FONT = 'system-ui, -apple-system, "Segoe UI", Roboto, sans-serif';

let theme = {};
let fills = [];
let inks = [];
let drawQueued = false;
let miniStale = true;
let dpr = 1;

function readTheme() {
  theme = {
    bg: cssVar("--bg"),
    bandA: cssVar("--band-a"),
    bandB: cssVar("--band-b"),
    district: cssVar("--district"),
    districtLine: cssVar("--district-line"),
    ink: cssVar("--ink"),
    muted: cssVar("--muted"),
    accent: cssVar("--accent"),
    hover: cssVar("--hover"),
  };
}

function recolor() {
  fills = buildings.map((b) => scoreColor(state.lens.value(b)));
  inks = buildings.map((b) => textOn(state.lens.value(b)));
  miniStale = true;
  requestDraw();
}

function requestDraw() {
  if (drawQueued) return;
  drawQueued = true;
  requestAnimationFrame(frame);
}

function frame(now) {
  drawQueued = false;
  if (flight) {
    flight(now);
    requestDraw();
  }
  draw();
}

function resize() {
  const box = canvas.getBoundingClientRect();
  dpr = window.devicePixelRatio || 1;
  view.w = Math.max(1, box.width);
  view.h = Math.max(1, box.height);
  canvas.width = Math.round(view.w * dpr);
  canvas.height = Math.round(view.h * dpr);
  sizeMinimap();
  requestDraw();
}

function screenRect(r) {
  return [(r.x - cam.x) * cam.s, (r.y - cam.y) * cam.s, r.w * cam.s, r.h * cam.s];
}

function onScreen(sx, sy, sw, sh) {
  return sx < view.w && sy < view.h && sx + sw > 0 && sy + sh > 0;
}

// Cheap width estimate; measureText for every label would dominate the frame on large repos.
function fitText(text, maxWidth, size) {
  const per = size * 0.58;
  const max = Math.floor(maxWidth / per);
  if (max < 3) return null;
  return text.length <= max ? text : text.slice(0, max - 1) + "…";
}

function draw() {
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.fillStyle = theme.bg;
  ctx.fillRect(0, 0, view.w, view.h);
  ctx.textBaseline = "middle";

  bands.forEach((band, i) => {
    const [sx, sy, sw, sh] = screenRect(band.rect);
    if (!onScreen(sx, sy, sw, sh)) return;
    ctx.fillStyle = i % 2 ? theme.bandB : theme.bandA;
    ctx.fillRect(sx, sy, sw, sh);
  });

  for (const d of districts) {
    const [sx, sy, sw, sh] = screenRect(d.rect);
    if (sw < 4 || sh < 4 || !onScreen(sx, sy, sw, sh)) continue;
    ctx.fillStyle = theme.district;
    ctx.fillRect(sx, sy, sw, sh);
    ctx.strokeStyle = theme.districtLine;
    ctx.lineWidth = 1;
    ctx.strokeRect(sx + 0.5, sy + 0.5, sw - 1, sh - 1);
  }

  for (let i = 0; i < buildings.length; i++) {
    const b = buildings[i];
    let [sx, sy, sw, sh] = screenRect(b.rect);
    if (!onScreen(sx, sy, sw, sh)) continue;
    sw = Math.max(sw, 1);
    sh = Math.max(sh, 1);
    ctx.globalAlpha = b.is_test ? 0.4 : 1;
    ctx.fillStyle = fills[i];
    ctx.fillRect(sx, sy, sw, sh);
    if (sw > 44 && sh > 16) {
      const size = Math.min(12, sh * 0.42);
      const text = fitText(b.label, sw - 8, size);
      if (text) {
        ctx.font = `${size}px ${FONT}`;
        ctx.fillStyle = inks[i];
        ctx.fillText(text, sx + 4, sy + Math.min(sh / 2, size + 2));
      }
    }
  }
  ctx.globalAlpha = 1;

  districts.forEach((d, i) => {
    const [sx, sy, sw, sh] = screenRect(d.rect);
    const strip = (d.inner.y - d.rect.y) * cam.s;
    if (sw < 48 || strip < 9 || !onScreen(sx, sy, sw, sh)) return;
    const size = Math.min(13, strip * 0.72);
    const score = d.maintainability === null ? "" : `  ${round(d.maintainability)}`;
    const text = fitText(districtLabel(i) + score, sw - 10, size);
    if (!text) return;
    ctx.font = `600 ${size}px ${FONT}`;
    ctx.fillStyle = theme.muted;
    ctx.fillText(text, sx + 5, sy + strip / 2 + 1);
  });

  bands.forEach((band) => {
    const [sx, sy, sw, sh] = screenRect(band.rect);
    if (!onScreen(sx, sy, sw, sh)) return;
    const strip = (districts[band.district].rect.y - band.rect.y) * cam.s;
    const size = clamp(strip * 0.6, 10, 15);
    ctx.font = `700 ${size}px ${FONT}`;
    ctx.fillStyle = theme.ink;
    const label = (band.layer || "unknown layer").toUpperCase();
    ctx.fillText(label, Math.max(sx, 0) + 8, sy + Math.max(strip, size + 4) / 2);
  });

  for (const i of state.matches) outline(buildings[i].rect, theme.accent, 2, [5, 3]);
  if (state.hover && state.hover.kind === "building") outline(buildings[state.hover.index].rect, theme.hover, 1.5);
  if (state.hover && state.hover.kind === "district") outline(districts[state.hover.index].rect, theme.hover, 1);
  if (state.selected) {
    const r = state.selected.kind === "building" ? buildings[state.selected.index].rect : districts[state.selected.index].rect;
    outline(r, theme.accent, 3);
  }
  drawMinimap();
}

function outline(r, color, width, dash) {
  let [sx, sy, sw, sh] = screenRect(r);
  if (!onScreen(sx, sy, sw, sh)) return;
  sw = Math.max(sw, 3);
  sh = Math.max(sh, 3);
  ctx.save();
  ctx.strokeStyle = color;
  ctx.lineWidth = width;
  if (dash) ctx.setLineDash(dash);
  ctx.strokeRect(sx - width / 2, sy - width / 2, sw + width, sh + width);
  ctx.restore();
}

const miniView = { w: 200, h: 120, s: 1 };

function sizeMinimap() {
  const b = MAP.bounds;
  const maxW = 200;
  const maxH = 160;
  const s = Math.min(maxW / Math.max(b.w, 1), maxH / Math.max(b.h, 1));
  miniView.s = s;
  miniView.w = Math.max(40, Math.round(b.w * s));
  miniView.h = Math.max(30, Math.round(b.h * s));
  mini.style.width = `${miniView.w}px`;
  mini.style.height = `${miniView.h}px`;
  mini.width = miniView.w * dpr;
  mini.height = miniView.h * dpr;
  miniBase.width = mini.width;
  miniBase.height = mini.height;
  miniStale = true;
}

function drawMinimap() {
  const [x0, y0] = toWorld(0, 0);
  const [x1, y1] = toWorld(view.w, view.h);
  const b = MAP.bounds;
  const allVisible = x0 <= b.x && y0 <= b.y && x1 >= b.x + b.w && y1 >= b.y + b.h;
  mini.style.visibility = allVisible ? "hidden" : "visible";
  if (allVisible) return;
  const s = miniView.s * dpr;
  if (miniStale) {
    const m = miniBase.getContext("2d");
    m.setTransform(1, 0, 0, 1, 0, 0);
    m.fillStyle = theme.bg;
    m.fillRect(0, 0, miniBase.width, miniBase.height);
    bands.forEach((band, i) => {
      m.fillStyle = i % 2 ? theme.bandB : theme.bandA;
      m.fillRect(band.rect.x * s, band.rect.y * s, band.rect.w * s, band.rect.h * s);
    });
    buildings.forEach((b, i) => {
      m.globalAlpha = b.is_test ? 0.4 : 1;
      m.fillStyle = fills[i];
      m.fillRect(b.rect.x * s, b.rect.y * s, Math.max(b.rect.w * s, 0.6), Math.max(b.rect.h * s, 0.6));
    });
    m.globalAlpha = 1;
    miniStale = false;
  }
  miniCtx.setTransform(1, 0, 0, 1, 0, 0);
  miniCtx.drawImage(miniBase, 0, 0);
  miniCtx.strokeStyle = theme.accent;
  miniCtx.lineWidth = 2 * dpr;
  miniCtx.strokeRect(x0 * s, y0 * s, (x1 - x0) * s, (y1 - y0) * s);
}

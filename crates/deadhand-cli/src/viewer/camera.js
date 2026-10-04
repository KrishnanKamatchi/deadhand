const view = { w: 1, h: 1 };
const cam = { x: 0, y: 0, s: 1 };
const zoomLimits = { min: 0.01, max: 50 };
const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
let flight = null;

function toScreen(x, y) {
  return [(x - cam.x) * cam.s, (y - cam.y) * cam.s];
}

function toWorld(px, py) {
  return [px / cam.s + cam.x, py / cam.s + cam.y];
}

// Camera that shows `r` filling `share` of the viewport, centred.
function framing(r, share) {
  const s = Math.min((view.w * share) / Math.max(r.w, 1e-6), (view.h * share) / Math.max(r.h, 1e-6));
  const scale = clamp(s, zoomLimits.min, zoomLimits.max);
  return {
    x: r.x + r.w / 2 - view.w / 2 / scale,
    y: r.y + r.h / 2 - view.h / 2 / scale,
    s: scale,
  };
}

function setLimits() {
  const fit = framing(MAP.bounds, 0.94).s;
  zoomLimits.min = fit * 0.5;
  zoomLimits.max = Math.max(fit * 2, 40);
}

function clamp(v, lo, hi) {
  return Math.max(lo, Math.min(hi, v));
}

function jumpTo(target) {
  flight = null;
  Object.assign(cam, target);
  requestDraw();
}

// Animates in screen-centre space with a logarithmic zoom, so long flights do not overshoot.
function flyTo(target) {
  if (reducedMotion) return jumpTo(target);
  const from = { ...cam };
  const centre = (c) => [c.x + view.w / 2 / c.s, c.y + view.h / 2 / c.s];
  const [fx, fy] = centre(from);
  const [tx, ty] = centre(target);
  const start = performance.now();
  const duration = 380;
  flight = (now) => {
    const t = Math.min(1, (now - start) / duration);
    const e = t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2;
    const s = Math.exp(Math.log(from.s) + (Math.log(target.s) - Math.log(from.s)) * e);
    const cx = fx + (tx - fx) * e;
    const cy = fy + (ty - fy) * e;
    cam.s = s;
    cam.x = cx - view.w / 2 / s;
    cam.y = cy - view.h / 2 / s;
    if (t >= 1) flight = null;
  };
  requestDraw();
}

function zoomAt(px, py, factor) {
  flight = null;
  const [wx, wy] = toWorld(px, py);
  cam.s = clamp(cam.s * factor, zoomLimits.min, zoomLimits.max);
  cam.x = wx - px / cam.s;
  cam.y = wy - py / cam.s;
  requestDraw();
}

function panBy(dx, dy) {
  flight = null;
  cam.x -= dx / cam.s;
  cam.y -= dy / cam.s;
  requestDraw();
}

function fitAll() {
  flyTo(framing(MAP.bounds, 0.94));
}

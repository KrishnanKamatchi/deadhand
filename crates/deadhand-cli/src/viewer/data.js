const MAP = JSON.parse(document.getElementById("map-data").textContent);
const buildings = MAP.buildings;
const districts = MAP.districts;
const bands = MAP.bands;
const rooms = MAP.rooms;

const districtChildren = districts.map(() => []);
const districtBuildings = districts.map(() => []);
districts.forEach((d, i) => {
  if (d.parent !== null && d.parent !== undefined) districtChildren[d.parent].push(i);
});
buildings.forEach((b, i) => districtBuildings[b.district].push(i));

const importsOf = buildings.map(() => []);
const importersOf = buildings.map(() => []);
for (const e of MAP.edges) {
  importsOf[e.from].push(e);
  importersOf[e.to].push(e);
}

const pinsOf = buildings.map(() => []);
for (const p of MAP.pins) pinsOf[p.building].push(p);

const LENSES = [{ id: "maintainability", label: "Maintainability", value: (b) => b.maintainability }];
for (const m of MAP.metrics) {
  if (m.available) LENSES.push({ id: m.kind, label: m.label, value: (b) => b.scores[m.kind] });
}

const state = {
  lens: LENSES[0],
  selected: null, // { kind: "building" | "district", index }
  hover: null,
  matches: new Set(),
};

// Red, through pale yellow, to blue (ColorBrewer RdYlBu): readable for the common forms of
// colour blindness, and the midpoint stays distinct instead of turning grey.
const RAMP = [
  [0, [215, 48, 39]],
  [25, [252, 141, 89]],
  [50, [254, 224, 144]],
  [75, [145, 191, 219]],
  [100, [69, 117, 180]],
];

function scoreRgb(score) {
  if (score === undefined || score === null || Number.isNaN(score)) return null;
  const s = Math.max(0, Math.min(100, score));
  for (let i = 1; i < RAMP.length; i++) {
    const [s1, c1] = RAMP[i];
    if (s <= s1) {
      const [s0, c0] = RAMP[i - 1];
      const t = (s - s0) / (s1 - s0);
      return c0.map((v, k) => Math.round(v + (c1[k] - v) * t));
    }
  }
  return RAMP[RAMP.length - 1][1];
}

function scoreColor(score) {
  const c = scoreRgb(score);
  return c ? `rgb(${c[0]},${c[1]},${c[2]})` : cssVar("--none");
}

function textOn(score) {
  const c = scoreRgb(score);
  if (!c) return "#1c1f1d";
  const lum = (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]) / 255;
  return lum > 0.55 ? "#1c1f1d" : "#ffffff";
}

function cssVar(name) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

function round(v) {
  return v === undefined || v === null ? "n/a" : String(Math.round(v));
}

function districtTrail(i) {
  const trail = [];
  for (let d = i; d !== null && d !== undefined; d = districts[d].parent) trail.unshift(d);
  return trail;
}

function districtLabel(i) {
  const d = districts[i];
  return d.path === "" ? MAP.root_name : d.label;
}

function bandName(band) {
  const layer = bands[band] && bands[band].layer;
  return layer ? layer : "unknown layer";
}

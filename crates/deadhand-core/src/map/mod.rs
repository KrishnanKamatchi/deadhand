//! A spatial model of a scanned repo for `deadhand map`.
//!
//! Layers become horizontal bands (in configured order, unknown last), directories become nested
//! districts inside a band, files become buildings and named functions become rooms inside a
//! building. Positions come from an ordered treemap over sorted paths, so the same repo always
//! gives the same map and a small change only moves its neighbours.

pub mod layout;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::Serialize;

use crate::config::Config;
use crate::evidence::{Finding, MetricKind, Severity};
use crate::layers::{self, Violation};
use crate::model::Span;
use crate::report::{MetricSummary, Summary};
use crate::Analysis;
use layout::{treemap, Rect};

/// Bumped on any breaking change to the map JSON shape.
pub const MAP_SCHEMA_VERSION: u32 = 1;

/// Map area per unit of file weight (one LOC ≈ a 10 × 10 square).
const AREA_PER_WEIGHT: f64 = 100.0;
/// Width ÷ height of the whole map.
const ASPECT: f64 = 1.6;
/// Files below this LOC still get this much area, so they stay visible and clickable.
const MIN_FILE_WEIGHT: f64 = 8.0;
/// Every band is at least this share of the map width tall, so its label stays readable.
const MIN_BAND_SHARE: f64 = 0.02;
/// Vertical space between bands, as a share of the map width.
const BAND_GAP_SHARE: f64 = 0.01;
/// Strip at the top of each band kept free for its label, as a share of the map width.
const BAND_LABEL_SHARE: f64 = 0.025;

/// The whole map. Indices in other entries refer to positions in these vectors.
#[derive(Debug, Clone, Serialize)]
pub struct MapModel {
    pub schema_version: u32,
    pub tool_version: String,
    pub root_name: String,
    pub summary: Summary,
    pub maintainability: f64,
    pub metrics: Vec<MetricSummary>,
    pub findings: Vec<Finding>,
    pub notes: Vec<String>,
    /// Configured layer order, top of the map first.
    pub layer_order: Vec<String>,
    pub bounds: Rect,
    pub bands: Vec<Band>,
    /// Pre-order: a district always comes after its parent.
    pub districts: Vec<District>,
    /// Same order as the scanned modules (sorted by path).
    pub buildings: Vec<Building>,
    pub rooms: Vec<Room>,
    pub edges: Vec<MapEdge>,
    /// Import cycles as building indices.
    pub cycles: Vec<Vec<usize>>,
    pub pins: Vec<Pin>,
}

/// One architectural layer, as a horizontal strip.
#[derive(Debug, Clone, Serialize)]
pub struct Band {
    /// `None` = files whose layer could not be detected.
    pub layer: Option<String>,
    pub rect: Rect,
    /// The district that fills this band.
    pub district: usize,
}

/// A directory inside one band. A directory with files in several layers appears once per band.
#[derive(Debug, Clone, Serialize)]
pub struct District {
    /// Repo-relative directory path (`""` = repo root).
    pub path: String,
    /// Display name. Chains of single-child directories are merged (`src/app/core`).
    pub label: String,
    pub band: usize,
    pub parent: Option<usize>,
    pub depth: u32,
    pub rect: Rect,
    /// Rectangle left for children after padding and the label strip.
    pub inner: Rect,
    pub loc: u64,
    pub files: u32,
    /// LOC-weighted Maintainability of non-test files; `None` if there are none.
    pub maintainability: Option<f64>,
}

/// One source file.
#[derive(Debug, Clone, Serialize)]
pub struct Building {
    pub path: String,
    /// File name.
    pub label: String,
    pub district: usize,
    pub layer: Option<String>,
    pub rect: Rect,
    pub is_test: bool,
    pub loc: u32,
    pub maintainability: f64,
    pub scores: BTreeMap<MetricKind, f64>,
    pub raw: BTreeMap<MetricKind, BTreeMap<String, f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<GitSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<f64>,
    pub exports: u32,
    pub external_packages: Vec<String>,
    /// This building's rooms are `rooms[first_room..first_room + room_count]`.
    pub first_room: usize,
    pub room_count: usize,
}

/// Git history of one file.
#[derive(Debug, Clone, Serialize)]
pub struct GitSummary {
    pub commits: u32,
    pub recent_commits: u32,
    pub authors: u32,
    /// Days between the file's last commit and the newest commit in the repo.
    pub days_since_last_commit: u32,
}

/// One named function inside a building.
#[derive(Debug, Clone, Serialize)]
pub struct Room {
    pub building: usize,
    pub name: String,
    pub span: Span,
    pub rect: Rect,
    pub loc: u32,
    pub params: u32,
    pub cyclomatic: u32,
    pub cognitive: u32,
    pub max_nesting: u32,
}

/// A deduplicated import between two buildings (test files have none).
#[derive(Debug, Clone, Serialize)]
pub struct MapEdge {
    pub from: usize,
    pub to: usize,
    pub type_only: bool,
    /// Both ends are in the same import cycle and the edge is a value import.
    pub in_cycle: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub violation: Option<Violation>,
}

/// A piece of evidence placed on the map.
#[derive(Debug, Clone, Serialize)]
pub struct Pin {
    pub building: usize,
    /// The innermost function that contains the evidence, when it has a location.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room: Option<usize>,
    pub metric: MetricKind,
    pub severity: Severity,
    pub key: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

/// Directory tree of the files in one band.
#[derive(Default)]
struct Dir<'a> {
    dirs: BTreeMap<&'a str, Dir<'a>>,
    /// Module indices.
    files: Vec<usize>,
    weight: f64,
}

/// Builds the map for an analysis.
pub fn build(analysis: &Analysis, cfg: &Config) -> MapModel {
    let report = analysis.report();
    let modules = &analysis.parsed.modules;

    let mut buildings: Vec<Building> = report
        .modules
        .iter()
        .map(|m| Building {
            path: m.path.clone(),
            label: m.path.rsplit('/').next().unwrap_or(&m.path).to_string(),
            district: 0,
            layer: m.layer.clone(),
            rect: Rect::default(),
            is_test: m.is_test,
            loc: m.loc,
            maintainability: m.maintainability,
            scores: m.scores.clone(),
            raw: m.raw.clone(),
            git: git_summary(analysis, &m.path),
            coverage: analysis.coverage.as_ref().and_then(|c| c.get(&m.path).copied()),
            exports: 0,
            external_packages: Vec::new(),
            first_room: 0,
            room_count: 0,
        })
        .collect();
    for (b, m) in buildings.iter_mut().zip(modules) {
        b.exports = m.exports;
        b.external_packages = m.external_packages().into_iter().map(String::from).collect();
    }

    let mut builder = Builder { buildings: &mut buildings, districts: Vec::new() };
    let (bounds, bands) = builder.layout_bands(&analysis.layers, &cfg.layers.order);
    let mut districts = builder.districts;
    district_stats(&mut districts, &buildings);
    let rooms = layout_rooms(&mut buildings, modules);
    let pins = place_pins(&report.evidence, &buildings, &rooms);

    let graph = &analysis.graph;
    let edges = graph
        .out
        .iter()
        .enumerate()
        .flat_map(|(i, outs)| {
            outs.iter().map(move |e| MapEdge {
                from: i,
                to: e.to,
                type_only: e.type_only,
                in_cycle: !e.type_only && graph.cycle_of[i].is_some() && graph.cycle_of[i] == graph.cycle_of[e.to],
                // Same rule as the report: type-only imports never break the layer order.
                violation: match (&analysis.layers[i], &analysis.layers[e.to]) {
                    (Some(f), Some(t)) if !e.type_only => layers::violation(f, t, &cfg.layers.order),
                    _ => None,
                },
            })
        })
        .collect();

    MapModel {
        schema_version: MAP_SCHEMA_VERSION,
        tool_version: report.tool_version,
        root_name: report.root_name,
        summary: report.summary,
        maintainability: report.maintainability,
        metrics: report.metrics,
        findings: report.findings,
        notes: report.notes,
        layer_order: cfg.layers.order.clone(),
        bounds,
        bands,
        districts,
        buildings,
        rooms,
        edges,
        cycles: graph.cycles.clone(),
        pins,
    }
}

fn git_summary(analysis: &Analysis, path: &str) -> Option<GitSummary> {
    let git = analysis.git.as_ref()?;
    let h = git.files.get(path)?;
    let days = (git.head_time - h.last_commit).max(0) / 86_400;
    Some(GitSummary {
        commits: h.commits,
        recent_commits: h.recent_commits,
        authors: h.authors,
        days_since_last_commit: u32::try_from(days).unwrap_or(u32::MAX),
    })
}

fn file_weight(loc: u32) -> f64 {
    f64::from(loc).max(MIN_FILE_WEIGHT)
}

struct Builder<'b> {
    buildings: &'b mut Vec<Building>,
    districts: Vec<District>,
}

impl Builder<'_> {
    /// Stacks one band per layer: configured order first, other named layers next, unknown last.
    fn layout_bands(&mut self, layer_of: &[Option<String>], order: &[String]) -> (Rect, Vec<Band>) {
        let present: BTreeSet<Option<&str>> = layer_of.iter().map(|l| l.as_deref()).collect();
        let mut band_layers: Vec<Option<&str>> =
            order.iter().map(|l| Some(l.as_str())).filter(|l| present.contains(l)).collect();
        band_layers.extend(present.iter().filter(|l| l.is_some_and(|l| !order.iter().any(|o| o == l))).copied());
        if present.contains(&None) {
            band_layers.push(None);
        }

        let mut trees: Vec<Dir<'_>> = Vec::with_capacity(band_layers.len());
        let paths: Vec<String> = self.buildings.iter().map(|b| b.path.clone()).collect();
        for layer in &band_layers {
            let mut root = Dir::default();
            for (i, l) in layer_of.iter().enumerate() {
                if l.as_deref() == *layer {
                    insert(&mut root, &paths[i], i, file_weight(self.buildings[i].loc));
                }
            }
            trees.push(root);
        }

        let total: f64 = trees.iter().map(|t| t.weight).sum();
        if total <= 0.0 {
            return (Rect::default(), Vec::new());
        }
        let width = (total * AREA_PER_WEIGHT * ASPECT).sqrt();
        let gap = width * BAND_GAP_SHARE;
        let mut y = 0.0;
        let mut bands = Vec::with_capacity(trees.len());
        for (b, (tree, layer)) in trees.iter().zip(&band_layers).enumerate() {
            // A small layer gets a compact block on the left instead of a full-width sliver; only
            // a layer too big for that spans the whole width. The area per LOC is the same
            // everywhere on the map.
            let area = tree.weight * AREA_PER_WEIGHT;
            let compact_width = (area * ASPECT).sqrt();
            let label = width * BAND_LABEL_SHARE;
            let content = if compact_width >= width {
                Rect::new(0.0, y + label, width, area / width)
            } else {
                Rect::new(0.0, y + label, compact_width, area / compact_width)
            };
            let height = (content.h + label).max(width * MIN_BAND_SHARE);
            let rect = Rect::new(0.0, y, width, height);
            let (dir, path, label) = collapse(tree, String::new(), String::new());
            let district = self.layout_dir(dir, path, label, content, None, 0, b);
            bands.push(Band { layer: layer.map(String::from), rect, district });
            y += height + gap;
        }
        (Rect::new(0.0, 0.0, width, y - gap), bands)
    }

    /// Lays out one district in `cell` and recurses into its children. Returns its index.
    #[allow(clippy::too_many_arguments)]
    fn layout_dir(
        &mut self,
        dir: &Dir<'_>,
        path: String,
        label: String,
        cell: Rect,
        parent: Option<usize>,
        depth: u32,
        band: usize,
    ) -> usize {
        let rect = cell.inset(cell.short() * 0.015, 0.0);
        let inner = rect.inset(rect.short() * 0.02, rect.short() * 0.06);
        let id = self.districts.len();
        self.districts.push(District {
            path: path.clone(),
            label: if label.is_empty() { ".".to_string() } else { label },
            band,
            parent,
            depth,
            rect,
            inner,
            loc: 0,
            files: 0,
            maintainability: None,
        });

        enum Item<'d, 'a> {
            Dir(&'a str, &'d Dir<'a>),
            File(usize),
        }
        let mut items: Vec<(&str, Item<'_, '_>)> = dir.dirs.iter().map(|(n, d)| (*n, Item::Dir(n, d))).collect();
        let names: Vec<String> = dir.files.iter().map(|&i| self.buildings[i].label.clone()).collect();
        items.extend(dir.files.iter().zip(&names).map(|(&i, n)| (n.as_str(), Item::File(i))));
        items.sort_by(|a, b| a.0.cmp(b.0));

        let weights: Vec<f64> = items
            .iter()
            .map(|(_, it)| match it {
                Item::Dir(_, d) => d.weight,
                Item::File(i) => file_weight(self.buildings[*i].loc),
            })
            .collect();
        for ((_, item), cell) in items.iter().zip(treemap(&weights, inner)) {
            match item {
                Item::Dir(name, d) => {
                    let (d, child_path, child_label) = collapse(d, join(&path, name), (*name).to_string());
                    self.layout_dir(d, child_path, child_label, cell, Some(id), depth + 1, band);
                }
                Item::File(i) => {
                    let b = &mut self.buildings[*i];
                    b.rect = cell.inset(cell.short() * 0.04, 0.0);
                    b.district = id;
                }
            }
        }
        id
    }
}

fn join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

fn insert<'a>(root: &mut Dir<'a>, path: &'a str, module: usize, weight: f64) {
    let mut node = root;
    node.weight += weight;
    let mut segments: Vec<&str> = path.split('/').collect();
    segments.pop();
    for seg in segments {
        node = node.dirs.entry(seg).or_default();
        node.weight += weight;
    }
    node.files.push(module);
}

/// Follows chains of directories that hold nothing but one subdirectory.
fn collapse<'d, 'a>(mut dir: &'d Dir<'a>, mut path: String, mut label: String) -> (&'d Dir<'a>, String, String) {
    while dir.files.is_empty() && dir.dirs.len() == 1 {
        let Some((name, child)) = dir.dirs.iter().next() else { break };
        path = join(&path, name);
        label = join(&label, name);
        dir = child;
    }
    (dir, path, label)
}

/// Fills LOC, file count and LOC-weighted Maintainability for every district.
fn district_stats(districts: &mut [District], buildings: &[Building]) {
    let mut weighted = vec![0.0; districts.len()];
    let mut scored_loc = vec![0.0; districts.len()];
    for b in buildings {
        let mut d = Some(b.district);
        while let Some(i) = d {
            districts[i].loc += u64::from(b.loc);
            districts[i].files += 1;
            if !b.is_test {
                let w = f64::from(b.loc.max(1));
                weighted[i] += b.maintainability * w;
                scored_loc[i] += w;
            }
            d = districts[i].parent;
        }
    }
    for (i, d) in districts.iter_mut().enumerate() {
        d.maintainability = (scored_loc[i] > 0.0).then(|| weighted[i] / scored_loc[i]);
    }
}

/// Places every named function inside its building, in source order.
fn layout_rooms(buildings: &mut [Building], modules: &[crate::model::ModuleFacts]) -> Vec<Room> {
    let mut rooms = Vec::new();
    for (i, (b, m)) in buildings.iter_mut().zip(modules).enumerate() {
        let mut fns: Vec<_> = m.functions.iter().collect();
        fns.sort_by(|a, c| a.span.cmp(&c.span).then_with(|| a.name.cmp(&c.name)));
        let inner = b.rect.inset(b.rect.short() * 0.05, b.rect.short() * 0.12);
        let weights: Vec<f64> = fns.iter().map(|f| f64::from(f.loc.max(1))).collect();
        b.first_room = rooms.len();
        b.room_count = fns.len();
        for (f, cell) in fns.into_iter().zip(treemap(&weights, inner)) {
            rooms.push(Room {
                building: i,
                name: f.name.clone(),
                span: f.span,
                rect: cell.inset(cell.short() * 0.05, 0.0),
                loc: f.loc,
                params: f.params,
                cyclomatic: f.cyclomatic,
                cognitive: f.cognitive,
                max_nesting: f.max_nesting,
            });
        }
    }
    rooms
}

/// Attaches each piece of evidence to its building and, when it has a line, its innermost room.
fn place_pins(evidence: &[crate::evidence::Evidence], buildings: &[Building], rooms: &[Room]) -> Vec<Pin> {
    let index: HashMap<&str, usize> = buildings.iter().enumerate().map(|(i, b)| (b.path.as_str(), i)).collect();
    evidence
        .iter()
        .filter_map(|e| {
            let &building = index.get(e.path.as_str())?;
            let b = &buildings[building];
            let room = e.span.and_then(|s| {
                (b.first_room..b.first_room + b.room_count)
                    .filter(|&r| rooms[r].span.start_line <= s.start_line && s.start_line <= rooms[r].span.end_line)
                    .min_by_key(|&r| (rooms[r].span.end_line - rooms[r].span.start_line, r))
            });
            Some(Pin {
                building,
                room,
                metric: e.metric,
                severity: e.severity,
                key: e.key.clone(),
                message: e.message.clone(),
                span: e.span,
            })
        })
        .collect()
}

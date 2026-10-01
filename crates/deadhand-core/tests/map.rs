//! `deadhand map` model checks against the fixture repos.

use std::path::PathBuf;

use deadhand_core::config::Config;
use deadhand_core::git::NoGit;
use deadhand_core::map::{self, MapModel};

const FIXTURES: [&str; 6] = ["clean", "cycles", "spaghetti", "deep-context", "drift", "aliases"];

fn build(name: &str) -> MapModel {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name);
    let cfg = Config::default();
    let analysis = deadhand_core::analyze_full(&root, &cfg, &NoGit).expect("scan");
    map::build(&analysis, &cfg)
}

#[test]
fn layout_nests_without_overlaps() {
    for name in FIXTURES {
        let m = build(name);
        assert!(!m.buildings.is_empty(), "{name}");
        for (i, d) in m.districts.iter().enumerate() {
            assert!(d.rect.contains(&d.inner), "{name}: district {i} inner");
            match d.parent {
                Some(p) => {
                    assert!(p < i, "{name}: district {i} comes before its parent");
                    assert!(m.districts[p].inner.contains(&d.rect), "{name}: district {} escapes parent", d.path);
                }
                None => assert!(m.bands[d.band].rect.contains(&d.rect), "{name}: root district {i} escapes band"),
            }
        }
        for b in &m.buildings {
            assert!(b.rect.w > 0.0 && b.rect.h > 0.0, "{name}: {} has no area", b.path);
            assert!(m.districts[b.district].inner.contains(&b.rect), "{name}: {} escapes district", b.path);
            let rooms = &m.rooms[b.first_room..b.first_room + b.room_count];
            for (k, r) in rooms.iter().enumerate() {
                assert!(b.rect.contains(&r.rect), "{name}: room {} escapes {}", r.name, b.path);
                assert!(
                    rooms[k + 1..].iter().all(|o| !o.rect.overlaps(&r.rect)),
                    "{name}: rooms overlap in {}",
                    b.path
                );
            }
        }
        // Siblings (buildings and districts sharing a parent) never overlap.
        let mut cells: Vec<(Option<usize>, &str, _)> =
            m.buildings.iter().map(|b| (Some(b.district), b.path.as_str(), b.rect)).collect();
        cells.extend(m.districts.iter().map(|d| (d.parent, d.path.as_str(), d.rect)));
        for (i, a) in cells.iter().enumerate() {
            for b in &cells[i + 1..] {
                if a.0.is_some() && a.0 == b.0 {
                    assert!(!a.2.overlaps(&b.2), "{name}: {} overlaps {}", a.1, b.1);
                }
            }
        }
        for (i, band) in m.bands.iter().enumerate() {
            assert!(m.bounds.contains(&band.rect), "{name}: band {i}");
            if let Some(next) = m.bands.get(i + 1) {
                assert!(!band.rect.overlaps(&next.rect), "{name}: bands {i} and {} overlap", i + 1);
            }
        }
    }
}

#[test]
fn bands_follow_layer_order_with_unknown_last() {
    let m = build("spaghetti");
    let layers: Vec<Option<&str>> = m.bands.iter().map(|b| b.layer.as_deref()).collect();
    assert_eq!(layers, vec![Some("ui"), Some("route"), Some("service"), Some("data"), Some("shared"), None]);
    let misc = m.districts.iter().find(|d| d.path == "src/misc").expect("src/misc");
    assert_eq!((misc.label.as_str(), misc.files), ("src/misc", 6));
}

#[test]
fn edges_mark_cycles_and_layer_violations() {
    let m = build("cycles");
    let groups: Vec<Vec<&str>> =
        m.cycles.iter().map(|c| c.iter().map(|&i| m.buildings[i].path.as_str()).collect()).collect();
    assert_eq!(groups, vec![vec!["src/a.ts", "src/b.ts", "src/c.ts"], vec!["src/d.ts", "src/e.ts"]]);
    assert!(m.edges.iter().filter(|e| e.in_cycle).all(|e| m.cycles.iter().any(|c| c.contains(&e.from))));

    let m = build("spaghetti");
    let path = |i: usize| m.buildings[i].path.as_str();
    let mut violations: Vec<(&str, &str)> =
        m.edges.iter().filter(|e| e.violation.is_some()).map(|e| (path(e.from), path(e.to))).collect();
    violations.sort();
    assert_eq!(
        violations,
        vec![
            ("src/components/OrderList.tsx", "src/db/client.ts"),
            ("src/db/client.ts", "src/routes/orders.ts"),
            ("src/routes/orders.ts", "src/db/client.ts"),
        ]
    );
}

#[test]
fn pins_land_on_their_building_and_function() {
    for name in FIXTURES {
        let m = build(name);
        let report_evidence = {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name);
            deadhand_core::analyze_with(&root, &Config::default(), &NoGit).unwrap().evidence.len()
        };
        assert_eq!(m.pins.len(), report_evidence, "{name}: every piece of evidence is pinned");
        for p in &m.pins {
            if let (Some(r), Some(span)) = (p.room, p.span) {
                let room = &m.rooms[r];
                assert_eq!(room.building, p.building);
                assert!(room.span.start_line <= span.start_line && span.start_line <= room.span.end_line);
            }
        }
    }
}

#[test]
fn districts_aggregate_their_files() {
    for name in FIXTURES {
        let m = build(name);
        for band in &m.bands {
            let root = &m.districts[band.district];
            let files = m.buildings.iter().filter(|b| b.layer == band.layer).count();
            assert_eq!(root.files as usize, files, "{name}: band {:?}", band.layer);
        }
    }
}

#[test]
fn maps_are_byte_identical() {
    for name in FIXTURES {
        let a = serde_json::to_string(&build(name)).unwrap();
        assert_eq!(a, serde_json::to_string(&build(name)).unwrap(), "{name}");
    }
}

#[test]
fn map_snapshots() {
    for name in ["spaghetti", "cycles"] {
        insta::assert_json_snapshot!(format!("map_{name}"), serde_json::to_value(build(name)).unwrap());
    }
}

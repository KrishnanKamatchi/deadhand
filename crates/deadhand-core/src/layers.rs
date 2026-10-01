//! Architectural layer detection from path segments, and layer-order checks.

use serde::Serialize;

use crate::config::LayerConfig;

/// Detects the layer of a repo-relative path. `None` = unknown.
///
/// A filename role suffix (`order.service.ts`, `user.controller.ts`) wins; otherwise the
/// deepest matching directory segment wins.
pub fn detect(path: &str, cfg: &LayerConfig) -> Option<String> {
    let mut segments: Vec<&str> = path.split('/').collect();
    let file = segments.pop().unwrap_or("");
    let file_roles: Vec<&str> = file.split('.').skip(1).collect(); // "order.service.ts" → ["service", "ts"]
    let lookup = |seg: &str| -> Option<String> {
        let seg = seg.to_ascii_lowercase();
        cfg.paths.iter().find(|(_, segs)| segs.contains(&seg)).map(|(layer, _)| layer.clone())
    };
    file_roles.iter().find_map(|r| lookup(r)).or_else(|| segments.iter().rev().find_map(|s| lookup(s)))
}

/// Why an import breaks the configured layer order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Violation {
    /// Imports a layer that sits above it (e.g. `data → route`).
    Inverted,
    /// Jumps over the `service` layer (e.g. `route → data`). The last layer is a foundation any layer may use.
    SkipsService,
}

/// Checks an import from layer `from` to layer `to` against `order` (left → right).
pub fn violation(from: &str, to: &str, order: &[String]) -> Option<Violation> {
    let pos = |l: &str| order.iter().position(|o| o == l);
    let (f, t) = (pos(from)?, pos(to)?);
    if t < f {
        return Some(Violation::Inverted);
    }
    let service = pos("service")?;
    let last = order.len() - 1;
    (f < service && t > service && t != last).then_some(Violation::SkipsService)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_layers() {
        let cfg = LayerConfig::default();
        assert_eq!(detect("src/components/Button.tsx", &cfg).as_deref(), Some("ui"));
        assert_eq!(detect("src/app/services/order.ts", &cfg).as_deref(), Some("service"));
        assert_eq!(detect("src/orders/order.service.ts", &cfg).as_deref(), Some("service"));
        assert_eq!(detect("src/utils/money.ts", &cfg).as_deref(), Some("shared"));
        assert_eq!(detect("src/index.ts", &cfg), None);
    }

    #[test]
    fn violations() {
        let order = LayerConfig::default().order;
        assert_eq!(violation("route", "data", &order), Some(Violation::SkipsService));
        assert_eq!(violation("ui", "data", &order), Some(Violation::SkipsService));
        assert_eq!(violation("data", "route", &order), Some(Violation::Inverted));
        assert_eq!(violation("route", "service", &order), None);
        assert_eq!(violation("ui", "infra", &order), None);
        assert_eq!(violation("shared", "data", &order), None);
    }
}

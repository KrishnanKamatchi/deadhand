//! Self-contained HTML viewer for `deadhand map`. Everything is embedded: no network, no server.

use anyhow::Result;
use deadhand_core::map::MapModel;

const TEMPLATE: &str = include_str!("../viewer/index.html");
const STYLE: &str = include_str!("../viewer/style.css");
const SCRIPTS: [&str; 4] = [
    include_str!("../viewer/data.js"),
    include_str!("../viewer/camera.js"),
    include_str!("../viewer/render.js"),
    include_str!("../viewer/ui.js"),
];

/// Renders the map as one HTML file with the model embedded as JSON.
pub fn map_html(map: &MapModel) -> Result<String> {
    let data = escape_for_script(&serde_json::to_string(map)?);
    let script = format!("(() => {{\n\"use strict\";\n{}\n}})();", SCRIPTS.join("\n"));
    // The data goes in last so nothing inside it can be mistaken for a placeholder.
    Ok(TEMPLATE
        .replacen("{{deadhand:style}}", STYLE.trim_end(), 1)
        .replacen("{{deadhand:script}}", &script, 1)
        .replacen("{{deadhand:data}}", &data, 1))
}

/// JSON inside `<script>` must not contain `</script>` or `<!--`. These escapes are valid JSON,
/// so the browser's `JSON.parse` reads the original strings back.
fn escape_for_script(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        match c {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use deadhand_core::config::Config;
    use deadhand_core::git::NoGit;

    use super::*;

    fn fixture_map(name: &str) -> MapModel {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name);
        let cfg = Config::default();
        let analysis = deadhand_core::analyze_full(&root, &cfg, &NoGit).unwrap();
        deadhand_core::map::build(&analysis, &cfg)
    }

    fn embedded_json(html: &str) -> &str {
        let start = html.find(r#"<script type="application/json" id="map-data">"#).unwrap();
        let rest = &html[start..];
        let open = rest.find('>').unwrap() + 1;
        let close = rest.find("</script>").unwrap();
        &rest[open..close]
    }

    #[test]
    fn embeds_the_model_and_viewer() {
        let map = fixture_map("spaghetti");
        let html = map_html(&map).unwrap();
        assert!(!html.contains("{{deadhand:"));
        // Parse both sides the same way: escaping must not change a single value.
        let embedded: serde_json::Value = serde_json::from_str(embedded_json(&html)).unwrap();
        let plain: serde_json::Value = serde_json::from_str(&serde_json::to_string(&map).unwrap()).unwrap();
        assert_eq!(embedded, plain);
        assert!(html.contains("function draw()"));
        assert!(!html.contains("http://") && !html.contains("https://"), "viewer must not load anything remote");
    }

    #[test]
    fn data_cannot_close_the_script_tag() {
        let escaped = escape_for_script(r#"{"path":"a</script><script>alert(1)</script>&<!--"}"#);
        assert!(!escaped.contains('<') && !escaped.contains('>') && !escaped.contains('&'));
        let back: serde_json::Value = serde_json::from_str(&escaped).unwrap();
        assert_eq!(back["path"], "a</script><script>alert(1)</script>&<!--");
    }

    #[test]
    fn output_is_deterministic() {
        let map = fixture_map("cycles");
        assert_eq!(map_html(&map).unwrap(), map_html(&fixture_map("cycles")).unwrap());
    }
}

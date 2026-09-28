//! File discovery: walks the repo once, honouring `.gitignore` and config globs.

use std::path::{Path, PathBuf};

use ignore::overrides::{Override, OverrideBuilder};
use ignore::WalkBuilder;

use crate::config::Config;
use crate::Error;

const EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"];
const SKIP_DIRS: &[&str] = &["node_modules", "dist", "build", ".next", "coverage", ".git"];

/// A file selected for analysis.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub abs: PathBuf,
    /// Repo-relative path with `/` separators.
    pub rel: String,
    pub is_test: bool,
}

/// Returns every JS/TS source file under `root`, sorted by relative path.
pub fn discover(root: &Path, cfg: &Config) -> Result<Vec<SourceFile>, Error> {
    let include = matcher(root, &cfg.include)?;
    let exclude = matcher(root, &cfg.exclude)?;

    let mut files = Vec::new();
    let walker = WalkBuilder::new(root)
        .require_git(false)
        .filter_entry(|e| {
            let is_dir = e.file_type().is_some_and(|t| t.is_dir());
            !(is_dir && e.depth() > 0 && SKIP_DIRS.iter().any(|d| e.file_name() == *d))
        })
        .build();

    for entry in walker {
        let entry = entry.map_err(|e| Error::Io(e.to_string()))?;
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !is_source_name(name) {
            continue;
        }
        let Ok(rel_path) = path.strip_prefix(root) else { continue };
        let rel = to_slash(rel_path);
        if let Some(inc) = &include {
            if !inc.matched(&rel, false).is_whitelist() {
                continue;
            }
        }
        if let Some(exc) = &exclude {
            if exc.matched(&rel, false).is_whitelist() {
                continue;
            }
        }
        files.push(SourceFile { is_test: is_test_path(&rel), abs: path.to_path_buf(), rel });
    }
    files.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(files)
}

/// JS/TS source by extension, excluding declaration files and minified bundles.
fn is_source_name(name: &str) -> bool {
    let Some((stem, ext)) = name.rsplit_once('.') else { return false };
    EXTENSIONS.contains(&ext)
        && !stem.ends_with(".d")
        && !stem.ends_with(".min")
}

/// Directories whose contents are test code, fixtures or mocks.
const TEST_DIRS: &[&str] = &["__tests__", "__mocks__", "__fixtures__", "fixtures", "test", "tests", "e2e"];

/// `*.test.*`, `*.spec.*`, or anything under a test/fixture/mock directory.
pub fn is_test_path(rel: &str) -> bool {
    let mut segments: Vec<&str> = rel.split('/').collect();
    let name = segments.pop().unwrap_or(rel);
    segments.iter().any(|s| TEST_DIRS.contains(s)) || name.contains(".test.") || name.contains(".spec.")
}

/// Detects an `@generated` marker in the file header.
pub fn is_generated(source: &str) -> bool {
    let end = source.len().min(1024);
    let head = source.get(..end).unwrap_or(source);
    head.contains("@generated")
}

fn matcher(root: &Path, globs: &[String]) -> Result<Option<Override>, Error> {
    if globs.is_empty() {
        return Ok(None);
    }
    let mut b = OverrideBuilder::new(root);
    for g in globs {
        b.add(g).map_err(|e| Error::Config(format!("invalid glob {g:?}: {e}")))?;
    }
    b.build().map(Some).map_err(|e| Error::Config(e.to_string()))
}

pub(crate) fn to_slash(p: &Path) -> String {
    let s = p.to_string_lossy();
    if std::path::MAIN_SEPARATOR == '/' {
        s.into_owned()
    } else {
        s.replace(std::path::MAIN_SEPARATOR, "/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_names() {
        assert!(is_source_name("a.ts"));
        assert!(is_source_name("a.tsx"));
        assert!(is_source_name("a.cjs"));
        assert!(!is_source_name("a.d.ts"));
        assert!(!is_source_name("a.min.js"));
        assert!(!is_source_name("a.json"));
        assert!(!is_source_name("a.vue"));
    }

    #[test]
    fn test_paths() {
        assert!(is_test_path("src/a.test.ts"));
        assert!(is_test_path("src/a.spec.tsx"));
        assert!(is_test_path("src/__tests__/a.ts"));
        assert!(is_test_path("test/fixtures/broken.ts"));
        assert!(is_test_path("src/__mocks__/fs.ts"));
        assert!(!is_test_path("src/testUtils.ts"));
        assert!(!is_test_path("src/testing.ts"));
    }
}

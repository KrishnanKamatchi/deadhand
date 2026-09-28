//! Import specifier → module resolution via `oxc_resolver` (tsconfig paths, index files, extensions).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use oxc_resolver::{ResolveError, ResolveOptions, Resolver, TsconfigDiscovery};

use crate::discover::to_slash;
use crate::model::ImportTarget;

/// Resolves specifiers from scanned files. Safe to share across threads.
pub struct ModuleResolver {
    resolver: Resolver,
    /// Same options without tsconfig, used when the tsconfig itself cannot be loaded
    /// (e.g. it `extends` a package that is not installed).
    fallback: Resolver,
    root: PathBuf,
    files: HashSet<String>,
}

impl ModuleResolver {
    /// `root` must be canonical; `files` are the repo-relative paths being scanned.
    pub fn new(root: &Path, files: impl IntoIterator<Item = String>) -> ModuleResolver {
        let ext = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let options = ResolveOptions {
            tsconfig: Some(TsconfigDiscovery::Auto),
            extensions: ext(&[".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs", ".json"]),
            extension_alias: vec![
                (".js".into(), ext(&[".ts", ".tsx", ".js", ".jsx"])),
                (".mjs".into(), ext(&[".mts", ".mjs"])),
                (".cjs".into(), ext(&[".cts", ".cjs"])),
            ],
            condition_names: ext(&["import", "require", "node", "default"]),
            builtin_modules: true,
            ..ResolveOptions::default()
        };
        let fallback = Resolver::new(ResolveOptions { tsconfig: None, ..options.clone() });
        ModuleResolver { resolver: Resolver::new(options), fallback, root: root.to_path_buf(), files: files.into_iter().collect() }
    }

    /// Resolves `specifier` as imported from the file at `from` (absolute).
    pub fn resolve(&self, from: &Path, specifier: &str) -> ImportTarget {
        let result = match self.resolver.resolve_file(from, specifier) {
            Err(e) if !matches!(e, ResolveError::NotFound(_) | ResolveError::Builtin { .. }) => {
                self.fallback.resolve_file(from, specifier)
            }
            other => other,
        };
        match result {
            Ok(res) => self.classify(res.path(), specifier),
            Err(ResolveError::Builtin { .. }) => ImportTarget::External(package_name(specifier)),
            Err(_) if is_local_looking(specifier) => ImportTarget::Unresolved,
            Err(_) => ImportTarget::External(package_name(specifier)),
        }
    }

    fn classify(&self, path: &Path, specifier: &str) -> ImportTarget {
        let Ok(rel) = path.strip_prefix(&self.root) else {
            return ImportTarget::External(package_name(specifier));
        };
        let rel = to_slash(rel);
        if rel.split('/').any(|s| s == "node_modules") {
            ImportTarget::External(package_name(specifier))
        } else if self.files.contains(&rel) {
            ImportTarget::Internal(rel)
        } else {
            ImportTarget::Unresolved
        }
    }
}

/// Relative, absolute, or a common alias prefix that should never be a package.
fn is_local_looking(s: &str) -> bool {
    s.starts_with('.') || s.starts_with('/') || s.starts_with("@/") || s.starts_with("~/") || s.starts_with('#')
}

/// `@scope/pkg/sub` → `@scope/pkg`, `pkg/sub` → `pkg`, `node:fs` → `node:fs`.
pub fn package_name(spec: &str) -> String {
    let mut parts = spec.split('/');
    let first = parts.next().unwrap_or(spec);
    match (first.starts_with('@'), parts.next()) {
        (true, Some(second)) => format!("{first}/{second}"),
        _ => first.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::package_name;

    #[test]
    fn package_names() {
        assert_eq!(package_name("react"), "react");
        assert_eq!(package_name("lodash/fp"), "lodash");
        assert_eq!(package_name("@prisma/client/runtime"), "@prisma/client");
        assert_eq!(package_name("node:fs"), "node:fs");
    }
}

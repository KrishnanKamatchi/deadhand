//! Plain-data facts extracted from source files. Nothing here borrows from the AST.

use serde::Serialize;

/// A 1-based line/column range in a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Span {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

/// How a module refers to another module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportKind {
    /// `import x from "y"` / `import "y"`
    Static,
    /// `import type { X } from "y"` (or every specifier is `type`)
    TypeOnly,
    /// `export ... from "y"`
    ReExport,
    /// `import("y")` with a string literal
    Dynamic,
    /// `require("y")` with a string literal
    Require,
}

impl ImportKind {
    /// Type-only imports vanish at runtime and are weighted lower.
    pub fn is_type_only(self) -> bool {
        matches!(self, ImportKind::TypeOnly)
    }
}

/// Where an import specifier points after resolution.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", content = "target", rename_all = "snake_case")]
pub enum ImportTarget {
    /// Another scanned module (repo-relative path).
    Internal(String),
    /// A package (bare specifier or anything under `node_modules`).
    External(String),
    /// A relative/aliased specifier that did not resolve to a scanned module.
    Unresolved,
}

/// One import statement, re-export or literal `import()`/`require()` call.
#[derive(Debug, Clone, Serialize)]
pub struct Import {
    pub specifier: String,
    pub kind: ImportKind,
    pub target: ImportTarget,
    pub line: u32,
}

/// Naming convention of an identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Casing {
    Camel,
    Pascal,
    Snake,
    ScreamingSnake,
    /// Single lowercase word: compatible with camelCase and snake_case.
    Lower,
    Other,
}

impl Casing {
    /// Classifies an identifier name, ignoring leading `_`/`$`.
    pub fn of(name: &str) -> Casing {
        let s = name.trim_start_matches(['_', '$']);
        let Some(first) = s.chars().next() else { return Casing::Other };
        let has_us = s.contains('_');
        let has_lower = s.chars().any(|c| c.is_ascii_lowercase());
        let has_upper = s.chars().any(|c| c.is_ascii_uppercase());
        match (first.is_ascii_uppercase(), has_us, has_lower, has_upper) {
            (_, _, false, true) => Casing::ScreamingSnake,
            (true, false, true, _) => Casing::Pascal,
            (false, true, true, false) => Casing::Snake,
            (false, false, true, true) => Casing::Camel,
            (false, false, true, false) => Casing::Lower,
            _ => Casing::Other,
        }
    }
}

/// Kind of a top-level symbol, used for "mixed naming within a file".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Variable,
    Class,
}

/// A top-level declared symbol.
#[derive(Debug, Clone, Serialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
}

/// Facts about one named function, method or named arrow function.
#[derive(Debug, Clone, Serialize)]
pub struct FunctionFacts {
    pub name: String,
    pub span: Span,
    pub loc: u32,
    pub params: u32,
    pub cyclomatic: u32,
    pub cognitive: u32,
    pub max_nesting: u32,
    /// Identifiers declared inside the function, excluding loop counters and
    /// parameters of inline callbacks (where short names are idiomatic).
    pub identifiers: Vec<String>,
}

/// Facts about one source file.
#[derive(Debug, Clone, Serialize)]
pub struct ModuleFacts {
    /// Repo-relative path with `/` separators.
    pub path: String,
    pub is_test: bool,
    /// Non-blank, non-comment lines.
    pub loc: u32,
    pub imports: Vec<Import>,
    pub exports: u32,
    pub top_level_decls: u32,
    pub functions: Vec<FunctionFacts>,
    /// Complexity of code outside any named function (module scope, anonymous callbacks at top level).
    pub top_level_cognitive: u32,
    pub symbols: Vec<Symbol>,
    pub parse_errors: Vec<String>,
}

impl ModuleFacts {
    /// Distinct external packages imported by this module.
    pub fn external_packages(&self) -> Vec<&str> {
        let mut v: Vec<&str> = self
            .imports
            .iter()
            .filter_map(|i| match &i.target {
                ImportTarget::External(p) => Some(p.as_str()),
                _ => None,
            })
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Sum of function cognitive complexity plus top-level complexity.
    pub fn total_cognitive(&self) -> u32 {
        self.top_level_cognitive + self.functions.iter().map(|f| f.cognitive).sum::<u32>()
    }
}

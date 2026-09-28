//! oxc parsing → [`ModuleFacts`]. One call per file; the AST never leaves this module.

mod lines;
mod visitor;

use std::cell::RefCell;

use oxc_allocator::Allocator;
use oxc_ast::ast::{BindingPattern, Declaration, Expression, Program, Statement};
use oxc_ast_visit::Visit;
use oxc_parser::{ParseOptions, Parser};
use oxc_span::SourceType;

use crate::model::{FunctionFacts, Import, ImportTarget, ModuleFacts, Symbol, SymbolKind};
pub use lines::Lines;
use visitor::Collector;

thread_local! {
    static ALLOCATOR: RefCell<Allocator> = RefCell::new(Allocator::default());
}

/// Parses `source` and extracts its facts. `resolve` maps an import specifier to its target.
///
/// Parse errors never abort: they are recorded on the returned facts.
pub fn parse_module(rel: &str, is_test: bool, source: &str, resolve: impl Fn(&str) -> ImportTarget) -> ModuleFacts {
    ALLOCATOR.with(|cell| {
        let mut allocator = cell.borrow_mut();
        allocator.reset();
        extract(&allocator, rel, is_test, source, resolve)
    })
}

fn source_type(rel: &str) -> SourceType {
    let st = SourceType::from_path(rel).unwrap_or_default();
    // .js files often contain JSX in React projects.
    if st.is_javascript() {
        st.with_jsx(true)
    } else {
        st
    }
}

fn extract(
    allocator: &Allocator,
    rel: &str,
    is_test: bool,
    source: &str,
    resolve: impl Fn(&str) -> ImportTarget,
) -> ModuleFacts {
    // CommonJS scripts may return at the top level.
    let options = ParseOptions {
        allow_return_outside_function: true,
        parse_regular_expression: false,
        ..ParseOptions::default()
    };
    let ret = Parser::new(allocator, source, source_type(rel)).with_options(options).parse();
    let parse_errors: Vec<String> = ret.diagnostics.iter().map(|d| d.to_string()).collect();
    let program = &ret.program;

    let comments: Vec<(u32, u32)> = program.comments.iter().map(|c| (c.span.start, c.span.end)).collect();
    let lines = Lines::new(source, &comments);

    let mut collector = Collector::new();
    collector.visit_program(program);

    let imports = collector
        .imports
        .iter()
        .map(|r| Import {
            target: resolve(&r.specifier),
            specifier: r.specifier.clone(),
            kind: r.kind,
            line: lines.line(r.offset),
        })
        .collect();

    let mut functions: Vec<FunctionFacts> = collector
        .functions
        .drain(..)
        .map(|f| FunctionFacts {
            name: f.name,
            span: lines.span(f.start, f.end),
            loc: lines.code_between(f.start, f.end),
            params: f.params,
            cyclomatic: f.cyclomatic,
            cognitive: f.cognitive,
            max_nesting: f.max_nesting,
            identifiers: f.identifiers,
        })
        .collect();
    functions.sort_by(|a, b| a.span.cmp(&b.span).then_with(|| a.name.cmp(&b.name)));

    let (symbols, top_level_decls) = top_level_symbols(program);

    ModuleFacts {
        path: rel.to_string(),
        is_test,
        loc: lines.total_code(),
        imports,
        exports: collector.exports,
        top_level_decls,
        functions,
        top_level_cognitive: collector.top_level_cognitive(),
        symbols,
        parse_errors,
    }
}

/// Top-level declared value symbols (for naming checks) and the count of all top-level declarations.
fn top_level_symbols(program: &Program<'_>) -> (Vec<Symbol>, u32) {
    let mut symbols = Vec::new();
    let mut count = 0u32;
    for stmt in &program.body {
        let decl = match stmt {
            Statement::ExportDeclaration(e) => Some(&e.declaration),
            Statement::ExportDefaultDeclaration(_) => {
                count += 1;
                None
            }
            other => other.as_declaration(),
        };
        let Some(decl) = decl else { continue };
        match decl {
            Declaration::FunctionDeclaration(f) => {
                count += 1;
                if let Some(id) = &f.id {
                    symbols.push(Symbol { name: id.name.to_string(), kind: SymbolKind::Function });
                }
            }
            Declaration::ClassDeclaration(c) => {
                count += 1;
                if let Some(id) = &c.id {
                    symbols.push(Symbol { name: id.name.to_string(), kind: SymbolKind::Class });
                }
            }
            Declaration::VariableDeclaration(v) => {
                for d in &v.declarations {
                    count += 1;
                    if let BindingPattern::BindingIdentifier(id) = &d.id {
                        let is_fn =
                            d.init.as_ref().is_some_and(|e: &Expression<'_>| e.get_inner_expression().is_function());
                        let kind = if is_fn { SymbolKind::Function } else { SymbolKind::Variable };
                        symbols.push(Symbol { name: id.name.to_string(), kind });
                    }
                }
            }
            _ => count += 1, // interfaces, type aliases, enums, namespaces
        }
    }
    (symbols, count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ImportKind;

    fn facts(src: &str) -> ModuleFacts {
        parse_module("src/x.tsx", false, src, |s| ImportTarget::External(s.to_string()))
    }

    fn cognitive(src: &str, name: &str) -> u32 {
        let f = facts(src);
        f.functions
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.cognitive)
            .unwrap_or_else(|| panic!("no fn {name}: {:?}", f.functions.iter().map(|f| &f.name).collect::<Vec<_>>()))
    }

    // Examples from the SonarSource Cognitive Complexity white paper, translated to TS.

    #[test]
    fn sonar_sum_of_primes() {
        let src = r#"
function sumOfPrimes(max: number): number {
  let total = 0;
  OUT: for (let i = 1; i <= max; ++i) {   // +1
    for (let j = 2; j < i; ++j) {         // +2 (nesting = 1)
      if (i % j == 0) {                   // +3 (nesting = 2)
        continue OUT;                     // +1
      }
    }
    total += i;
  }
  return total;
}"#;
        assert_eq!(cognitive(src, "sumOfPrimes"), 7);
    }

    #[test]
    fn sonar_get_words() {
        let src = r#"
function getWords(n: number): string {
  switch (n) {          // +1
    case 1: return "one";
    case 2: return "a couple";
    case 3: return "a few";
    default: return "lots";
  }
}"#;
        assert_eq!(cognitive(src, "getWords"), 1);
        assert_eq!(facts(src).functions[0].cyclomatic, 4);
    }

    #[test]
    fn sonar_boolean_sequences() {
        let src = "function f(a,b,c,d,e,f2){ if (a && b && c || d || e && f2) {} }";
        // if +1, && run +1, || run +1, && run +1
        assert_eq!(cognitive(src, "f"), 4);
    }

    #[test]
    fn sonar_nested_lambda_and_else() {
        let src = r#"
function run(items: number[]) {
  items.forEach((x) => {        // nesting +1 (anonymous)
    if (x > 1) {                // +2
      log(x);
    } else if (x < 0) {         // +1
      log(-x);
    } else {                    // +1
      log(0);
    }
  });
  try { risky(); } catch (e) {  // +1
    throw e;
  }
}"#;
        assert_eq!(cognitive(src, "run"), 5);
    }

    #[test]
    fn blueprint_example() {
        let src = r#"
export function applyDiscount(order: Order, user: User) {
  if (!user) return 0;
  let total = 0;
  for (const item of order.items) {
    if (item.onSale && user.isMember) {
      total += item.price * 0.8;
    } else {
      total += item.price;
    }
  }
  return total > 100 ? total - 10 : total;
}"#;
        let f = &facts(src).functions[0];
        assert_eq!((f.cognitive, f.cyclomatic, f.max_nesting, f.params, f.loc), (7, 6, 2, 2, 12));
        assert_eq!(f.identifiers, vec!["order", "user", "total"]);
    }

    #[test]
    fn recursion_counts_once() {
        assert_eq!(cognitive("function fact(n){ return n <= 1 ? 1 : n * fact(n - 1); }", "fact"), 2);
    }

    #[test]
    fn names_arrows_methods_and_wrapped_components() {
        let src = r#"
const add = (a: number, b: number) => a + b;
export const Button = forwardRef((props, ref) => null);
class Cart { total() { return 0; } handler = () => 1; }
const api = { load() { return 1; } };
export default function () {}
"#;
        let names: Vec<_> = facts(src).functions.into_iter().map(|f| f.name).collect();
        assert_eq!(names, vec!["add", "Button", "Cart.total", "Cart.handler", "load", "default"]);
    }

    #[test]
    fn imports_of_every_kind() {
        let src = r#"
import a from "./a";
import type { B } from "./b";
import { type C } from "./c";
export * from "./d";
export { e } from "./e";
const f = require("./f");
const g = () => import("./g");
import h = require("./h");
"#;
        let kinds: Vec<_> = facts(src).imports.into_iter().map(|i| (i.specifier, i.kind)).collect();
        use ImportKind::*;
        assert_eq!(
            kinds,
            vec![
                ("./a".into(), Static),
                ("./b".into(), TypeOnly),
                ("./c".into(), TypeOnly),
                ("./d".into(), ReExport),
                ("./e".into(), ReExport),
                ("./f".into(), Require),
                ("./g".into(), Dynamic),
                ("./h".into(), Require),
            ]
        );
    }

    #[test]
    fn exports_and_symbols() {
        let f = facts(
            "export const a = 1, b = 2; export function c() {} export default 3; module.exports.d = 4; interface I {}",
        );
        assert_eq!(f.exports, 5);
        assert_eq!(f.top_level_decls, 5);
        assert_eq!(f.symbols.len(), 3);
    }

    #[test]
    fn commonjs_top_level_return_is_valid() {
        let f = parse_module("scripts/x.js", false, "if (!process.argv[2]) { return; }\nmodule.exports = 1;", |s| {
            ImportTarget::External(s.into())
        });
        assert!(f.parse_errors.is_empty(), "{:?}", f.parse_errors);
    }

    #[test]
    fn parse_errors_do_not_panic() {
        let f = facts("function (");
        assert!(!f.parse_errors.is_empty());
    }

    #[test]
    fn loop_counters_are_exempt() {
        let f = facts("function f(list){ for (let i = 0; i < 3; i++) {} for (const x of list) {} const tmp = 1; }");
        assert_eq!(f.functions[0].identifiers, vec!["list", "tmp"]);
    }
}

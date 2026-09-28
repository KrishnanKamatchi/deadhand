//! Single-pass AST visitor: imports, exports and per-function complexity.
//!
//! Cognitive complexity follows the SonarSource spec:
//! - +1 (plus current nesting) for `if`, ternary, `switch`, loops, `catch`
//! - +1 flat for `else if`, `else`, labelled `break`/`continue`, direct recursion
//! - +1 per run of identical boolean operators (`a && b && c || d` = 2)
//! - nesting grows inside those structures and inside anonymous callbacks.
//!
//! Named functions (declarations, methods, arrows assigned to a name) get their own
//! score. Anonymous callbacks count toward the enclosing function, one level deeper,
//! exactly like lambdas in the spec.

use oxc_ast::ast::*;
use oxc_ast_visit::{walk, Visit};
use oxc_syntax::scope::ScopeFlags;

use crate::model::ImportKind;

/// An import found in the source before resolution.
pub struct RawImport {
    pub specifier: String,
    pub kind: ImportKind,
    pub offset: u32,
}

/// A finished named function, still in byte offsets.
pub struct RawFunction {
    pub name: String,
    pub start: u32,
    pub end: u32,
    pub params: u32,
    pub cyclomatic: u32,
    pub cognitive: u32,
    pub max_nesting: u32,
    pub identifiers: Vec<String>,
}

struct Frame {
    name: Option<String>,
    start: u32,
    end: u32,
    params: u32,
    cyclomatic: u32,
    cognitive: u32,
    nesting: u32,
    max_nesting: u32,
    identifiers: Vec<String>,
}

impl Frame {
    fn new(name: Option<String>, start: u32, end: u32, params: u32) -> Frame {
        Frame { name, start, end, params, cyclomatic: 1, cognitive: 0, nesting: 0, max_nesting: 0, identifiers: Vec::new() }
    }
}

/// Collects facts while walking one program.
pub struct Collector {
    frames: Vec<Frame>,
    pub functions: Vec<RawFunction>,
    pub imports: Vec<RawImport>,
    pub exports: u32,
    /// Names waiting for the function that starts at the given offset.
    pending: Vec<(u32, String)>,
    classes: Vec<Option<String>>,
    /// >0 while visiting bindings where short names are idiomatic.
    exempt: u32,
    /// Set just before entering an arrow function so its parameters are exempt.
    exempt_params: bool,
}

impl Collector {
    pub fn new() -> Collector {
        Collector {
            frames: vec![Frame::new(None, 0, 0, 0)],
            functions: Vec::new(),
            imports: Vec::new(),
            exports: 0,
            pending: Vec::new(),
            classes: Vec::new(),
            exempt: 0,
            exempt_params: false,
        }
    }

    /// Cognitive complexity of code outside named functions.
    pub fn top_level_cognitive(&self) -> u32 {
        self.frames.first().map_or(0, |f| f.cognitive)
    }

    fn top(&mut self) -> &mut Frame {
        let i = self.frames.len() - 1;
        &mut self.frames[i]
    }

    /// Structural increment: +1 plus nesting, and one more path.
    fn structural(&mut self) {
        let f = self.top();
        f.cognitive += 1 + f.nesting;
        f.cyclomatic += 1;
    }

    fn flat(&mut self, n: u32) {
        self.top().cognitive += n;
    }

    fn nested(&mut self, walk: impl FnOnce(&mut Self)) {
        let i = self.frames.len() - 1;
        let f = &mut self.frames[i];
        f.nesting += 1;
        f.max_nesting = f.max_nesting.max(f.nesting);
        walk(self);
        self.frames[i].nesting -= 1;
    }

    fn exempt(&mut self, walk: impl FnOnce(&mut Self)) {
        self.exempt += 1;
        walk(self);
        self.exempt -= 1;
    }

    fn name_function_at(&mut self, expr: &Expression<'_>, name: String) {
        match expr.get_inner_expression() {
            Expression::FunctionExpression(f) => self.pending.push((f.span.start, name)),
            Expression::ArrowFunctionExpression(f) => self.pending.push((f.span.start, name)),
            // `const Button = forwardRef((props, ref) => ...)`, `memo(...)`, `useCallback(...)`
            Expression::CallExpression(call) => {
                if let Some(arg) = call.arguments.iter().find_map(|a| a.as_expression().filter(|e| e.get_inner_expression().is_function())) {
                    self.name_function_at(arg, name);
                }
            }
            _ => {}
        }
    }

    fn take_name(&mut self, start: u32) -> Option<String> {
        let pos = self.pending.iter().rposition(|(s, _)| *s == start)?;
        Some(self.pending.swap_remove(pos).1)
    }

    /// Runs `walk` inside a new frame for a named function, or one nesting level
    /// deeper in the current frame for an anonymous one.
    fn function_scope(&mut self, name: Option<String>, start: u32, end: u32, params: &FormalParameters<'_>, walk: impl FnOnce(&mut Self)) {
        let Some(name) = name else {
            self.exempt_params = false;
            self.nested(|v| v.exempt(|v| v.visit_formal_parameters(params)));
            self.nested(walk);
            return;
        };
        let count = params.items.len() as u32 + u32::from(params.rest.is_some());
        self.frames.push(Frame::new(Some(name), start, end, count));
        if std::mem::take(&mut self.exempt_params) {
            self.exempt(|v| v.visit_formal_parameters(params));
        } else {
            self.visit_formal_parameters(params);
        }
        walk(self);
        if let Some(f) = self.frames.pop() {
            self.functions.push(RawFunction {
                name: f.name.unwrap_or_default(),
                start: f.start,
                end: f.end,
                params: f.params,
                cyclomatic: f.cyclomatic,
                cognitive: f.cognitive,
                max_nesting: f.max_nesting,
                identifiers: f.identifiers,
            });
        }
    }

    fn push_import(&mut self, specifier: &str, kind: ImportKind, offset: u32) {
        self.imports.push(RawImport { specifier: specifier.to_string(), kind, offset });
    }

    /// `if` → `else if` → `else` chain: only the first `if` gets a nesting increment.
    fn walk_if_chain(&mut self, it: &IfStatement<'_>) {
        self.visit_expression(&it.test);
        self.nested(|v| v.visit_statement(&it.consequent));
        match &it.alternate {
            Some(Statement::IfStatement(elif)) => {
                self.flat(1);
                self.top().cyclomatic += 1;
                self.walk_if_chain(elif);
            }
            Some(alt) => {
                self.flat(1);
                self.nested(|v| v.visit_statement(alt));
            }
            None => {}
        }
    }
}

fn literal_source<'b>(e: &'b Expression<'_>) -> Option<&'b str> {
    match e.get_inner_expression() {
        Expression::StringLiteral(s) => Some(s.value.as_str()),
        Expression::TemplateLiteral(t) if t.expressions.is_empty() => t.quasis.first().map(|q| q.value.raw.as_str()),
        _ => None,
    }
}

fn collect_logical<'b, 'a>(e: &'b Expression<'a>, ops: &mut Vec<LogicalOperator>, leaves: &mut Vec<&'b Expression<'a>>) {
    match e.without_parentheses() {
        Expression::LogicalExpression(l) => {
            collect_logical(&l.left, ops, leaves);
            ops.push(l.operator);
            collect_logical(&l.right, ops, leaves);
        }
        other => leaves.push(other),
    }
}

fn is_module_exports(target: &AssignmentTarget<'_>) -> bool {
    let Some(member) = target.as_member_expression() else { return false };
    match member.object() {
        Expression::Identifier(id) => id.name == "exports" || (id.name == "module" && member.static_property_name() == Some("exports")),
        Expression::StaticMemberExpression(inner) => {
            matches!(&inner.object, Expression::Identifier(id) if id.name == "module") && inner.property.name == "exports"
        }
        _ => false,
    }
}

impl<'a> Visit<'a> for Collector {
    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        let all_type = it.specifiers.as_ref().is_some_and(|s| {
            !s.is_empty()
                && s.iter().all(|sp| matches!(sp, ImportDeclarationSpecifier::ImportSpecifier(x) if x.import_kind.is_type()))
        });
        let kind = if it.import_kind.is_type() || all_type { ImportKind::TypeOnly } else { ImportKind::Static };
        self.push_import(it.source.value.as_str(), kind, it.span.start);
    }

    fn visit_export_declaration(&mut self, it: &ExportDeclaration<'a>) {
        self.exports += match &it.declaration {
            Declaration::VariableDeclaration(v) => v.declarations.len() as u32,
            _ => 1,
        };
        walk::walk_export_declaration(self, it);
    }

    fn visit_export_named_declaration(&mut self, it: &ExportNamedDeclaration<'a>) {
        self.exports += it.specifiers.len() as u32;
    }

    fn visit_export_from_declaration(&mut self, it: &ExportFromDeclaration<'a>) {
        let kind = if it.export_kind.is_type() { ImportKind::TypeOnly } else { ImportKind::ReExport };
        self.push_import(it.source.value.as_str(), kind, it.span.start);
        self.exports += it.specifiers.len() as u32;
    }

    fn visit_export_default_declaration(&mut self, it: &ExportDefaultDeclaration<'a>) {
        self.exports += 1;
        match &it.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(f) if f.id.is_none() => {
                self.pending.push((f.span.start, "default".into()));
            }
            ExportDefaultDeclarationKind::ArrowFunctionExpression(f) => {
                self.pending.push((f.span.start, "default".into()));
            }
            _ => {}
        }
        walk::walk_export_default_declaration(self, it);
    }

    fn visit_export_all_declaration(&mut self, it: &ExportAllDeclaration<'a>) {
        let kind = if it.export_kind.is_type() { ImportKind::TypeOnly } else { ImportKind::ReExport };
        self.push_import(it.source.value.as_str(), kind, it.span.start);
        self.exports += 1;
    }

    fn visit_ts_import_equals_declaration(&mut self, it: &TSImportEqualsDeclaration<'a>) {
        if let TSModuleReference::ExternalModuleReference(r) = &it.module_reference {
            let kind = if it.import_kind.is_type() { ImportKind::TypeOnly } else { ImportKind::Require };
            self.push_import(r.expression.value.as_str(), kind, it.span.start);
        }
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        if let Some(src) = literal_source(&it.source) {
            self.push_import(src, ImportKind::Dynamic, it.span.start);
        }
        walk::walk_import_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if it.is_require_call() {
            if let Some(src) = it.arguments.first().and_then(|a| a.as_expression()).and_then(literal_source) {
                self.push_import(src, ImportKind::Require, it.span.start);
            }
        }
        if let Expression::Identifier(id) = &it.callee {
            let recursive = self.frames.len() > 1 && self.frames.last().and_then(|f| f.name.as_deref()) == Some(id.name.as_str());
            if recursive {
                self.flat(1);
            }
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        if is_module_exports(&it.left) {
            self.exports += 1;
        }
        if let Some(name) = it.left.get_identifier_name() {
            self.name_function_at(&it.right, name.to_string());
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let (BindingPattern::BindingIdentifier(id), Some(init)) = (&it.id, &it.init) {
            self.name_function_at(init, id.name.to_string());
        }
        walk::walk_variable_declarator(self, it);
    }

    fn visit_class(&mut self, it: &Class<'a>) {
        self.classes.push(it.id.as_ref().map(|id| id.name.to_string()));
        walk::walk_class(self, it);
        self.classes.pop();
    }

    fn visit_method_definition(&mut self, it: &MethodDefinition<'a>) {
        if let Some(key) = it.key.name() {
            let name = match self.classes.last() {
                Some(Some(class)) => format!("{class}.{key}"),
                _ => key.into_owned(),
            };
            self.pending.push((it.value.span.start, name));
        }
        walk::walk_method_definition(self, it);
    }

    fn visit_property_definition(&mut self, it: &PropertyDefinition<'a>) {
        if let (Some(key), Some(value)) = (it.key.name(), &it.value) {
            let name = match self.classes.last() {
                Some(Some(class)) => format!("{class}.{key}"),
                _ => key.into_owned(),
            };
            self.name_function_at(value, name);
        }
        walk::walk_property_definition(self, it);
    }

    fn visit_object_property(&mut self, it: &ObjectProperty<'a>) {
        if let Some(key) = it.key.static_name() {
            self.name_function_at(&it.value, key.into_owned());
        }
        walk::walk_object_property(self, it);
    }

    fn visit_function(&mut self, it: &Function<'a>, _flags: ScopeFlags) {
        let name = self.take_name(it.span.start).or_else(|| it.id.as_ref().map(|id| id.name.to_string()));
        self.function_scope(name, it.span.start, it.span.end, &it.params, |v| {
            if let Some(body) = &it.body {
                v.visit_function_body(body);
            }
        });
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        let name = self.take_name(it.span.start);
        self.exempt_params = true;
        self.function_scope(name, it.span.start, it.span.end, &it.params, |v| v.visit_arrow_function_body(&it.body));
    }

    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        if self.exempt == 0 && self.frames.len() > 1 {
            self.top().identifiers.push(it.name.to_string());
        }
    }

    fn visit_if_statement(&mut self, it: &IfStatement<'a>) {
        self.structural();
        self.walk_if_chain(it);
    }

    fn visit_conditional_expression(&mut self, it: &ConditionalExpression<'a>) {
        self.structural();
        self.visit_expression(&it.test);
        self.nested(|v| {
            v.visit_expression(&it.consequent);
            v.visit_expression(&it.alternate);
        });
    }

    fn visit_switch_statement(&mut self, it: &SwitchStatement<'a>) {
        let f = self.top();
        f.cognitive += 1 + f.nesting;
        f.cyclomatic += it.cases.iter().filter(|c| c.test.is_some()).count() as u32;
        self.visit_expression(&it.discriminant);
        self.nested(|v| {
            for case in &it.cases {
                v.visit_switch_case(case);
            }
        });
    }

    fn visit_for_statement(&mut self, it: &ForStatement<'a>) {
        self.structural();
        if let Some(init) = &it.init {
            self.exempt(|v| v.visit_for_statement_init(init));
        }
        if let Some(test) = &it.test {
            self.visit_expression(test);
        }
        if let Some(update) = &it.update {
            self.visit_expression(update);
        }
        self.nested(|v| v.visit_statement(&it.body));
    }

    fn visit_for_in_statement(&mut self, it: &ForInStatement<'a>) {
        self.structural();
        self.exempt(|v| v.visit_for_statement_left(&it.left));
        self.visit_expression(&it.right);
        self.nested(|v| v.visit_statement(&it.body));
    }

    fn visit_for_of_statement(&mut self, it: &ForOfStatement<'a>) {
        self.structural();
        self.exempt(|v| v.visit_for_statement_left(&it.left));
        self.visit_expression(&it.right);
        self.nested(|v| v.visit_statement(&it.body));
    }

    fn visit_while_statement(&mut self, it: &WhileStatement<'a>) {
        self.structural();
        self.visit_expression(&it.test);
        self.nested(|v| v.visit_statement(&it.body));
    }

    fn visit_do_while_statement(&mut self, it: &DoWhileStatement<'a>) {
        self.structural();
        self.nested(|v| v.visit_statement(&it.body));
        self.visit_expression(&it.test);
    }

    fn visit_catch_clause(&mut self, it: &CatchClause<'a>) {
        self.structural();
        if let Some(param) = &it.param {
            self.exempt(|v| v.visit_catch_parameter(param));
        }
        self.nested(|v| v.visit_block_statement(&it.body));
    }

    fn visit_break_statement(&mut self, it: &BreakStatement<'a>) {
        if it.label.is_some() {
            self.flat(1);
        }
    }

    fn visit_continue_statement(&mut self, it: &ContinueStatement<'a>) {
        if it.label.is_some() {
            self.flat(1);
        }
    }

    fn visit_logical_expression(&mut self, it: &LogicalExpression<'a>) {
        let mut ops = Vec::new();
        let mut leaves = Vec::new();
        collect_logical(&it.left, &mut ops, &mut leaves);
        ops.push(it.operator);
        collect_logical(&it.right, &mut ops, &mut leaves);
        let runs = 1 + ops.windows(2).filter(|w| w[0] != w[1]).count() as u32;
        self.flat(runs);
        self.top().cyclomatic += ops.len() as u32;
        for leaf in leaves {
            self.visit_expression(leaf);
        }
    }
}

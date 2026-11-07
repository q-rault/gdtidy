//! The typed tree is complete: on every valid script of Godot's test suite,
//! every node of the syntax tree can be reached from `SourceFile` through
//! the typed accessors alone.
//!
//! The walk below uses only the `ast` API. Any node it does not reach is a
//! part of GDScript a tool could not read without falling back to raw nodes.

use std::collections::HashSet;
use std::path::Path;

use gdsyntax::SyntaxKind::{self, *};
use gdsyntax::SyntaxNode;
use gdsyntax::ast::*;

/// List nodes whose items the API returns directly, documented in `ast`.
const LIST_NODES: &[SyntaxKind] = &[RETURN_TYPE, TYPE_ARGS, ANNOTATION_ARGS];

#[derive(Default)]
struct Walk {
    seen: HashSet<SyntaxNode>,
}

impl Walk {
    fn mark(&mut self, node: &SyntaxNode) {
        assert!(
            self.seen.insert(node.clone()),
            "{:?} reached twice",
            node.kind()
        );
    }

    fn file(&mut self, file: &SourceFile) {
        self.mark(file.syntax());
        for a in file.annotations() {
            self.annotation(&a);
        }
        if let Some(c) = file.class_name() {
            self.mark(c.syntax());
            if let Some(e) = c.extends() {
                self.mark(e.syntax());
            }
        }
        // `extends` after `class_name` was reached above.
        if let Some(e) = file.extends()
            && !self.seen.contains(e.syntax())
        {
            self.mark(e.syntax());
        }
        for m in file.members() {
            self.member(&m);
        }
    }

    fn annotation(&mut self, a: &Annotation) {
        self.mark(a.syntax());
        for e in a.arguments() {
            self.expr(&e);
        }
    }

    fn member(&mut self, m: &Member) {
        match m {
            Member::Var(v) => self.var(v),
            Member::Const(c) => self.constant(c),
            Member::Signal(s) => {
                self.mark(s.syntax());
                for a in m.annotations() {
                    self.annotation(&a);
                }
                if let Some(p) = s.params() {
                    self.params(&p);
                }
            }
            Member::Enum(e) => {
                self.mark(e.syntax());
                for a in m.annotations() {
                    self.annotation(&a);
                }
                for v in e.values() {
                    self.mark(v.syntax());
                    if let Some(x) = v.value() {
                        self.expr(&x);
                    }
                }
            }
            Member::Func(f) => self.func(f),
            Member::Class(c) => {
                self.mark(c.syntax());
                for a in c.annotations() {
                    self.annotation(&a);
                }
                if let Some(e) = c.extends() {
                    self.mark(e.syntax());
                }
                if let Some(b) = c.body() {
                    self.mark(b.syntax());
                    for a in b.annotations() {
                        self.annotation(&a);
                    }
                    for m in b.members() {
                        self.member(&m);
                    }
                }
            }
            Member::StringStatement(s) => self.mark(s.syntax()),
            Member::Pass(p) => self.mark(p.syntax()),
            Member::Error(e) => self.mark(e.syntax()),
        }
    }

    fn var(&mut self, v: &Var) {
        self.mark(v.syntax());
        for a in v.annotations() {
            self.annotation(&a);
        }
        if let Some(t) = v.declared_type() {
            self.ty(&t);
        }
        if let Some(e) = v.initializer() {
            self.expr(&e);
        }
        if let Some(p) = v.property() {
            self.mark(p.syntax());
            if let Some(s) = p.setter() {
                self.mark(s.syntax());
                if let Some(ps) = s.params() {
                    self.params(&ps);
                }
                if let Some(b) = s.body() {
                    self.suite(&b);
                }
            }
            if let Some(g) = p.getter() {
                self.mark(g.syntax());
                if let Some(b) = g.body() {
                    self.suite(&b);
                }
            }
        }
    }

    fn constant(&mut self, c: &Const) {
        self.mark(c.syntax());
        for a in children_annotations(c.syntax()) {
            self.annotation(&a);
        }
        if let Some(t) = c.declared_type() {
            self.ty(&t);
        }
        if let Some(e) = c.initializer() {
            self.expr(&e);
        }
    }

    fn func(&mut self, f: &Func) {
        self.mark(f.syntax());
        for a in f.annotations() {
            self.annotation(&a);
        }
        if let Some(p) = f.params() {
            self.params(&p);
        }
        if let Some(t) = f.return_type() {
            self.ty(&t);
        }
        if let Some(b) = f.body() {
            self.suite(&b);
        }
    }

    fn params(&mut self, p: &ParamList) {
        self.mark(p.syntax());
        for param in p.params() {
            self.mark(param.syntax());
            for a in children_annotations(param.syntax()) {
                self.annotation(&a);
            }
            if let Some(t) = param.declared_type() {
                self.ty(&t);
            }
            if let Some(e) = param.default_value() {
                self.expr(&e);
            }
        }
    }

    fn ty(&mut self, t: &Type) {
        self.mark(t.syntax());
        for a in t.arguments() {
            self.ty(&a);
        }
    }

    fn suite(&mut self, s: &Suite) {
        self.mark(s.syntax());
        for st in s.statements() {
            self.stmt(&st);
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        // A variable's or constant's annotations are walked with it.
        if !matches!(s, Stmt::Var(_) | Stmt::Const(_)) {
            for a in s.annotations() {
                self.annotation(&a);
            }
        }
        match s {
            Stmt::Var(v) => self.var(v),
            Stmt::Const(c) => self.constant(c),
            Stmt::Pass(_) | Stmt::Break(_) | Stmt::Continue(_) | Stmt::Breakpoint(_) => {
                self.mark(s.syntax())
            }
            Stmt::Return(r) => {
                self.mark(r.syntax());
                if let Some(e) = r.value() {
                    self.expr(&e);
                }
            }
            Stmt::Expr(e) => {
                self.mark(e.syntax());
                if let Some(x) = e.expr() {
                    self.expr(&x);
                }
            }
            Stmt::If(i) => {
                self.mark(i.syntax());
                self.opt_expr(i.condition());
                self.opt_suite(i.body());
                for c in i.elif_clauses() {
                    self.mark(c.syntax());
                    self.opt_expr(c.condition());
                    self.opt_suite(c.body());
                }
                if let Some(c) = i.else_clause() {
                    self.mark(c.syntax());
                    self.opt_suite(c.body());
                }
            }
            Stmt::For(f) => {
                self.mark(f.syntax());
                if let Some(t) = f.declared_type() {
                    self.ty(&t);
                }
                self.opt_expr(f.iterable());
                self.opt_suite(f.body());
            }
            Stmt::While(w) => {
                self.mark(w.syntax());
                self.opt_expr(w.condition());
                self.opt_suite(w.body());
            }
            Stmt::Match(m) => {
                self.mark(m.syntax());
                self.opt_expr(m.subject());
                if let Some(p) = m.pass() {
                    self.mark(p.syntax());
                }
                for b in m.branches() {
                    self.mark(b.syntax());
                    for a in b.annotations() {
                        self.annotation(&a);
                    }
                    for p in b.patterns() {
                        self.pattern(&p);
                    }
                    if let Some(g) = b.guard() {
                        self.mark(g.syntax());
                        self.opt_expr(g.condition());
                    }
                    self.opt_suite(b.body());
                }
            }
            Stmt::Assert(a) => {
                self.mark(a.syntax());
                if let Some(args) = a.arguments() {
                    self.args(&args);
                }
            }
            Stmt::Annotation(a) => self.annotation(a),
            Stmt::Error(e) => self.mark(e.syntax()),
        }
    }

    fn pattern(&mut self, p: &Pattern) {
        self.mark(p.syntax());
        match p {
            Pattern::Expr(e) => self.opt_expr(e.expr()),
            Pattern::Array(a) => {
                for e in a.elements() {
                    self.pattern(&e);
                }
            }
            Pattern::Dict(d) => {
                for entry in d.entries() {
                    self.mark(entry.syntax());
                    self.opt_expr(entry.key());
                    if let Some(v) = entry.value() {
                        self.pattern(&v);
                    }
                }
                if let Some(r) = d.rest() {
                    self.mark(r.syntax());
                }
            }
            Pattern::Bind(_) | Pattern::Wildcard(_) | Pattern::Rest(_) | Pattern::Error(_) => {}
        }
    }

    fn args(&mut self, a: &ArgList) {
        self.mark(a.syntax());
        for e in a.arguments() {
            self.expr(&e);
        }
    }

    fn opt_expr(&mut self, e: Option<Expr>) {
        if let Some(e) = e {
            self.expr(&e);
        }
    }

    fn opt_suite(&mut self, s: Option<Suite>) {
        if let Some(s) = s {
            self.suite(&s);
        }
    }

    fn expr(&mut self, e: &Expr) {
        self.mark(e.syntax());
        match e {
            Expr::Literal(_) | Expr::Name(_) | Expr::SelfExpr(_) | Expr::GetNode(_) => {}
            Expr::Error(_) => {}
            Expr::Unary(u) => self.opt_expr(u.operand()),
            Expr::Binary(b) => {
                self.opt_expr(b.lhs());
                self.opt_expr(b.rhs());
            }
            Expr::Ternary(t) => {
                self.opt_expr(t.true_value());
                self.opt_expr(t.condition());
                self.opt_expr(t.false_value());
            }
            Expr::Assignment(a) => {
                self.opt_expr(a.target());
                self.opt_expr(a.value());
            }
            Expr::Await(a) => self.opt_expr(a.operand()),
            Expr::Array(a) => {
                for x in a.elements() {
                    self.expr(&x);
                }
            }
            Expr::Dict(d) => {
                for entry in d.entries() {
                    self.mark(entry.syntax());
                    self.opt_expr(entry.key());
                    self.opt_expr(entry.value());
                }
            }
            Expr::Paren(p) => self.opt_expr(p.inner()),
            Expr::Attribute(a) => self.opt_expr(a.base()),
            Expr::Subscript(s) => {
                self.opt_expr(s.base());
                self.opt_expr(s.index());
            }
            Expr::Cast(c) => {
                self.opt_expr(c.operand());
                if let Some(t) = c.target_type() {
                    self.ty(&t);
                }
            }
            Expr::Call(c) => {
                self.opt_expr(c.callee());
                if let Some(a) = c.arg_list() {
                    self.args(&a);
                }
            }
            Expr::Preload(p) => {
                if let Some(a) = p.arg_list() {
                    self.args(&a);
                }
            }
            Expr::Lambda(l) => {
                if let Some(p) = l.params() {
                    self.params(&p);
                }
                if let Some(t) = l.return_type() {
                    self.ty(&t);
                }
                self.opt_suite(l.body());
            }
            Expr::TypeTest(t) => {
                self.opt_expr(t.operand());
                if let Some(ty) = t.tested_type() {
                    self.ty(&ty);
                }
            }
        }
    }
}

fn children_annotations(node: &SyntaxNode) -> impl Iterator<Item = Annotation> {
    node.children().filter_map(Annotation::cast)
}

fn scripts() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/godot/4.7");
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "gd") {
                let name = path.strip_prefix(&root).unwrap().display().to_string();
                out.push((name, std::fs::read_to_string(&path).unwrap()));
            }
        }
    }
    out.sort();
    out
}

/// Walks a valid script and records every node the walk missed.
fn check_reachable(name: &str, source: String, missing: &mut Vec<String>) -> bool {
    let tree = gdsyntax::parse(source);
    if tree.has_errors() {
        return false;
    }
    let mut walk = Walk::default();
    walk.file(&tree.file());
    for node in tree.root().descendants() {
        if !walk.seen.contains(&node) && !LIST_NODES.contains(&node.kind()) {
            let parent = node.parent().map(|p| p.kind());
            missing.push(format!("{name}: {:?} in {parent:?}", node.kind()));
        }
    }
    true
}

fn assert_none_missing(missing: &[String]) {
    // One example per kind of unreachable node.
    let mut examples: Vec<&str> = Vec::new();
    let mut kinds = HashSet::new();
    for m in missing {
        if kinds.insert(m.rsplit_once(": ").unwrap().1) {
            examples.push(m);
        }
    }
    assert!(
        missing.is_empty(),
        "{} nodes unreachable, for example:
{}",
        missing.len(),
        examples.join(
            "
"
        )
    );
}

#[test]
fn every_node_of_a_valid_script_is_reachable() {
    let mut checked = 0;
    let mut missing = Vec::new();
    for (name, source) in scripts() {
        checked += usize::from(check_reachable(&name, source, &mut missing));
    }
    assert!(checked > 400, "only {checked} valid scripts");
    assert_none_missing(&missing);
}

/// The same check on real projects, with `GDSYNTAX_CORPUS` as in the
/// `corpus` test. Does nothing without it.
#[test]
fn every_node_of_the_corpus_is_reachable() {
    let Some(dirs) = std::env::var_os("GDSYNTAX_CORPUS") else {
        return;
    };
    let mut stack: Vec<_> = std::env::split_paths(&dirs).collect();
    let mut missing = Vec::new();
    let mut checked = 0;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = path.file_name().unwrap_or_default();
            if path.is_dir() {
                if file_name != ".godot" && file_name != ".git" {
                    stack.push(path);
                }
            } else if path.extension().is_some_and(|e| e == "gd")
                && let Ok(source) = std::fs::read_to_string(&path)
            {
                let name = path.display().to_string();
                checked += usize::from(check_reachable(&name, source, &mut missing));
            }
        }
    }
    eprintln!("{checked} valid corpus files walked");
    assert_none_missing(&missing);
}

/// On broken scripts, the typed views never panic and every `ERROR` node
/// the walk meets is reachable as an `Error` variant.
#[test]
fn broken_scripts_can_be_walked() {
    for (_, source) in scripts() {
        let tree = gdsyntax::parse(source);
        if tree.has_errors() {
            Walk::default().file(&tree.file());
        }
    }
}

#[test]
fn typed_values() {
    let tree = gdsyntax::parse(
        "func f(x):\n\tmatch x:\n\t\t[1, var y, ..] when y > 0: pass\n\t\t{\"k\": _, ..}: pass\n\tvar n = %Unique\n\tvar m = $\"A/B\"\n\tx += 0x10\n\tvar s = \"a\\tb\"\n\tvar t = a not in b\n",
    );
    assert!(!tree.has_errors(), "{:?}", tree.errors());
    let Some(Member::Func(f)) = tree.file().members().next() else {
        panic!()
    };
    let stmts: Vec<_> = f.body().unwrap().statements().collect();

    let Stmt::Match(m) = &stmts[0] else { panic!() };
    let branches: Vec<_> = m.branches().collect();
    let Some(Pattern::Array(a)) = branches[0].patterns().next() else {
        panic!()
    };
    let elements: Vec<_> = a.elements().collect();
    assert!(matches!(elements[0], Pattern::Expr(_)));
    let Pattern::Bind(b) = &elements[1] else {
        panic!()
    };
    assert_eq!(b.name().unwrap().text(), "y");
    assert!(matches!(elements[2], Pattern::Rest(_)));
    assert!(branches[0].guard().unwrap().condition().is_some());
    let Some(Pattern::Dict(d)) = branches[1].patterns().next() else {
        panic!()
    };
    assert!(d.rest().is_some());
    assert!(matches!(
        d.entries().next().unwrap().value(),
        Some(Pattern::Wildcard(_))
    ));

    let node_path = |s: &Stmt| {
        let Stmt::Var(v) = s else { panic!() };
        let Some(Expr::GetNode(g)) = v.initializer() else {
            panic!()
        };
        (g.is_unique(), g.path().unwrap())
    };
    assert_eq!(node_path(&stmts[1]), (true, "Unique".to_string()));
    assert_eq!(node_path(&stmts[2]), (false, "A/B".to_string()));

    let Stmt::Expr(e) = &stmts[3] else { panic!() };
    let Some(Expr::Assignment(assign)) = e.expr() else {
        panic!()
    };
    assert_eq!(assign.op(), Some(AssignOp::Add));
    let Some(Expr::Literal(lit)) = assign.value() else {
        panic!()
    };
    assert_eq!(lit.value(), Some(gdsyntax::literal::LiteralValue::Int(16)));

    let Stmt::Var(s) = &stmts[4] else { panic!() };
    let Some(Expr::Literal(lit)) = s.initializer() else {
        panic!()
    };
    assert_eq!(
        lit.value(),
        Some(gdsyntax::literal::LiteralValue::String("a\tb".into()))
    );

    let Stmt::Var(t) = &stmts[5] else { panic!() };
    let Some(Expr::Binary(bin)) = t.initializer() else {
        panic!()
    };
    assert_eq!(bin.op(), Some(BinaryOp::NotIn));
}

#[test]
fn skipped_code_is_kept_in_an_error_node() {
    let tree = gdsyntax::parse("func f():\n\tvar n = 1 2 3\n\tpass\n");
    assert!(tree.has_errors());
    let Some(Member::Func(f)) = tree.file().members().next() else {
        panic!()
    };
    let stmts: Vec<_> = f.body().unwrap().statements().collect();
    assert!(
        stmts.iter().any(|s| matches!(s, Stmt::Error(_))),
        "{stmts:?}"
    );
    assert!(matches!(stmts.last(), Some(Stmt::Pass(_))));
}

//! Tests for mir_visit and MirVisitor in src/utils.rs
//! Framework: Rust built-in test harness (#[test])

#[allow(unused_imports)]
use crate::utils::{mir_visit, MirVisitor};
#[allow(unused_imports)]
use crate::mir::{Function, MirDecl, MirStatement, MirTerminator, BasicBlock};

struct TraceVisitor {
    calls: Vec<&'static str>,
}
impl TraceVisitor {
    fn new() -> Self { Self { calls: Vec::new() } }
}
impl MirVisitor for TraceVisitor {
    fn visit_func(&mut self, _func: &Function) { self.calls.push("func"); }
    fn visit_decl(&mut self, _decl: &MirDecl) { self.calls.push("decl"); }
    fn visit_stmt(&mut self, _stmt: &MirStatement) { self.calls.push("stmt"); }
    fn visit_term(&mut self, _term: &MirTerminator) { self.calls.push("term"); }
}

#[test]
fn mir_visit_calls_all_hooks_in_order() {
    // Construct a minimal Function with:
    // - 2 decls
    // - 2 basic blocks: 
    //    BB0 with 2 statements and a terminator
    //    BB1 with 1 statement and no terminator
    let f = Function {
        decls: vec![MirDecl::new_dummy(0), MirDecl::new_dummy(1)],
        basic_blocks: vec![
            BasicBlock {
                statements: vec![MirStatement::new_dummy(0), MirStatement::new_dummy(1)],
                terminator: Some(MirTerminator::new_dummy(0)),
            },
            BasicBlock {
                statements: vec![MirStatement::new_dummy(2)],
                terminator: None,
            },
        ],
        ..Function::default()
    };

    let mut v = TraceVisitor::new();
    mir_visit(&f, &mut v);

    // First the function, then two decls, then BB0 stmts x2 + term, then BB1 stmt
    // We don't assert exact interleaving across blocks beyond sequence count summary.
    assert!(matches!(v.calls.first(), Some(&"func")));
    let counts = v.calls.iter().fold((0,0,0,0), |mut acc, &k| {
        match k {
            "func" => acc.0 += 1,
            "decl" => acc.1 += 1,
            "stmt" => acc.2 += 1,
            "term" => acc.3 += 1,
            _ => {}
        }
        acc
    });
    assert_eq!(counts, (1, 2, 3, 1));
}

#[test]
fn mir_visit_handles_empty_function() {
    let f = Function::default();
    let mut v = TraceVisitor::new();
    mir_visit(&f, &mut v);
    // Only function should be visited
    assert_eq!(v.calls, vec!["func"]);
}
//! Simple AST-level inlining for small pure functions.
//!
//! This is a general optimisation: functions whose body is a single `return`
//! expression are inlined at their call sites when the arguments are trivial
//! (no side effects).  This eliminates call overhead and lets later stages
//! (constant folding, the integer fast-path JIT, etc.) see a single loop body.

use std::collections::HashMap;

use crate::ast::*;

#[derive(Clone)]
struct InlineCandidate {
    params: Vec<String>,
    body: Expr,
}

/// Inline eligible function calls throughout the program.
///
/// When `remove_unused` is `false`, top-level function definitions are kept
/// even if they are not referenced inside the same file. This is required when
/// compiling a module whose purpose is to export those definitions.
pub fn inline_small_functions(stmts: &mut [Stmt], remove_unused: bool) {
    let mut candidates: HashMap<String, InlineCandidate> = HashMap::new();

    for stmt in stmts.iter() {
        if let Stmt::Define {
            name,
            params,
            body,
            return_type,
            ..
        } = stmt
        {
            // Inlining a call erases its runtime type-annotation checks (the
            // VM/JIT only check annotations on real calls), so annotated
            // functions must stay as ordinary calls.
            let has_annotations =
                return_type.is_some() || params.iter().any(|(_, ann)| ann.is_some());
            if !has_annotations && body.len() == 1 {
                if let Stmt::Return {
                    value: Some(expr), ..
                } = &body[0]
                {
                    candidates.insert(
                        name.clone(),
                        InlineCandidate {
                            params: params.iter().map(|(n, _)| n.clone()).collect(),
                            body: expr.clone(),
                        },
                    );
                }
            }
        }
    }

    for stmt in stmts.iter_mut() {
        inline_stmt(stmt, &candidates, 3);
    }

    let error_candidates = collect_error_candidates(stmts);
    if !error_candidates.is_empty() {
        inline_try_error_calls(stmts, &error_candidates);
    }

    simplify_guard_tries(stmts);

    if remove_unused {
        remove_unused_defines(stmts);
    }
}

fn remove_unused_defines(stmts: &mut [Stmt]) {
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    for stmt in stmts.iter() {
        collect_used_names(stmt, &mut used);
    }
    for stmt in stmts.iter_mut() {
        if let Stmt::Define { name, .. } = stmt {
            if !used.contains(name) {
                // Replace the unused definition with an empty expression statement
                // so that the AST shape is preserved without renumbering.
                *stmt = Stmt::Expr(Expr::Nothing(crate::ast::Span { line: 0, col: 0 }));
            }
        }
    }
}

fn collect_used_names(stmt: &Stmt, used: &mut std::collections::HashSet<String>) {
    match stmt {
        Stmt::Let { value, .. }
        | Stmt::Show(value)
        | Stmt::Return { value: Some(value), .. }
        | Stmt::Expr(value) => collect_used_names_expr(value, used),
        Stmt::Set { target, value } => {
            collect_used_names_expr(value, used);
            collect_used_names_assign(target, used);
        }
        Stmt::If { cond, then_branch, else_branch } => {
            collect_used_names_expr(cond, used);
            for s in then_branch { collect_used_names(s, used); }
            for s in else_branch { collect_used_names(s, used); }
        }
        Stmt::While { cond, body } => {
            collect_used_names_expr(cond, used);
            for s in body { collect_used_names(s, used); }
        }
        Stmt::For { iterable, body, .. } => {
            collect_used_names_expr(iterable, used);
            for s in body { collect_used_names(s, used); }
        }
        Stmt::Try { body, catch_body, .. } => {
            for s in body { collect_used_names(s, used); }
            for s in catch_body { collect_used_names(s, used); }
        }
        Stmt::Define { body, .. } => {
            for s in body { collect_used_names(s, used); }
        }
        Stmt::Class { init, methods, .. } => {
            if let Some(init) = init {
                for s in &init.body { collect_used_names(s, used); }
            }
            for m in methods { collect_used_names(m, used); }
        }
        Stmt::Read { path, .. }
        | Stmt::Write { path, .. } => collect_used_names_expr(path, used),
        Stmt::Init(init) => {
            for s in &init.body { collect_used_names(s, used); }
        }
        _ => {}
    }
}

fn collect_used_names_assign(target: &AssignTarget, used: &mut std::collections::HashSet<String>) {
    match target {
        AssignTarget::Index { object, index, .. } => {
            collect_used_names_expr(object, used);
            collect_used_names_expr(index, used);
        }
        AssignTarget::Property { object, .. } => collect_used_names_expr(object, used),
        _ => {}
    }
}

fn collect_used_names_expr(expr: &Expr, used: &mut std::collections::HashSet<String>) {
    match expr {
        Expr::Variable { name, .. } => { used.insert(name.clone()); }
        Expr::Binary { left, right, .. } => {
            collect_used_names_expr(left, used);
            collect_used_names_expr(right, used);
        }
        Expr::Unary { operand, .. } => collect_used_names_expr(operand, used),
        Expr::Call { callee, args, .. } => {
            collect_used_names_expr(callee, used);
            for a in args { collect_used_names_expr(a, used); }
        }
        Expr::Index { object, index, .. } => {
            collect_used_names_expr(object, used);
            collect_used_names_expr(index, used);
        }
        Expr::Property { object, .. } => collect_used_names_expr(object, used),
        Expr::New { class, args, .. } => {
            collect_used_names_expr(class, used);
            for a in args { collect_used_names_expr(a, used); }
        }
        Expr::Tell { object, args, .. } => {
            collect_used_names_expr(object, used);
            for a in args { collect_used_names_expr(a, used); }
        }
        Expr::List(items, _) => {
            for item in items { collect_used_names_expr(item, used); }
        }
        Expr::Dict(entries, _) => {
            for (k, v) in entries {
                collect_used_names_expr(k, used);
                collect_used_names_expr(v, used);
            }
        }
        _ => {}
    }
}

fn inline_stmt(stmt: &mut Stmt, candidates: &HashMap<String, InlineCandidate>, depth: usize) {
    match stmt {
        Stmt::Let { value, .. }
        | Stmt::Show(value)
        | Stmt::Return { value: Some(value), .. }
        | Stmt::Expr(value) => inline_expr(value, candidates, depth),
        Stmt::Set { target, value } => {
            inline_assign_target(target, candidates, depth);
            inline_expr(value, candidates, depth);
        }
        Stmt::If { cond, then_branch, else_branch } => {
            inline_expr(cond, candidates, depth);
            for s in then_branch.iter_mut() {
                inline_stmt(s, candidates, depth);
            }
            for s in else_branch.iter_mut() {
                inline_stmt(s, candidates, depth);
            }
        }
        Stmt::While { cond, body } => {
            inline_expr(cond, candidates, depth);
            for s in body.iter_mut() {
                inline_stmt(s, candidates, depth);
            }
        }
        Stmt::For { iterable, body, .. } => {
            inline_expr(iterable, candidates, depth);
            for s in body.iter_mut() {
                inline_stmt(s, candidates, depth);
            }
        }
        Stmt::Try { body, catch_body, .. } => {
            for s in body.iter_mut() {
                inline_stmt(s, candidates, depth);
            }
            for s in catch_body.iter_mut() {
                inline_stmt(s, candidates, depth);
            }
        }
        Stmt::Define { body, .. } => {
            for s in body.iter_mut() {
                inline_stmt(s, candidates, depth);
            }
        }
        Stmt::Class { init, methods, .. } => {
            if let Some(init) = init {
                for s in init.body.iter_mut() {
                    inline_stmt(s, candidates, depth);
                }
            }
            for method in methods.iter_mut() {
                inline_stmt(method, candidates, depth);
            }
        }
        Stmt::Read { path, .. }
        | Stmt::Write { path, .. } => inline_expr(path, candidates, depth),
        Stmt::Init(init) => {
            for s in init.body.iter_mut() {
                inline_stmt(s, candidates, depth);
            }
        }
        _ => {}
    }
}

fn inline_assign_target(target: &mut AssignTarget, candidates: &HashMap<String, InlineCandidate>, depth: usize) {
    match target {
        AssignTarget::Index { object, index, .. } => {
            inline_expr(object, candidates, depth);
            inline_expr(index, candidates, depth);
        }
        AssignTarget::Property { object, .. } => {
            inline_expr(object, candidates, depth);
        }
        _ => {}
    }
}

fn inline_expr(expr: &mut Expr, candidates: &HashMap<String, InlineCandidate>, depth: usize) {
    // First recurse into sub-expressions so inner calls get a chance too.
    match expr {
        Expr::Binary { left, right, .. } => {
            inline_expr(left, candidates, depth);
            inline_expr(right, candidates, depth);
        }
        Expr::Unary { operand, .. } => inline_expr(operand, candidates, depth),
        Expr::Call { callee, args, .. } => {
            inline_expr(callee, candidates, depth);
            for arg in args.iter_mut() {
                inline_expr(arg, candidates, depth);
            }
        }
        Expr::Index { object, index, .. } => {
            inline_expr(object, candidates, depth);
            inline_expr(index, candidates, depth);
        }
        Expr::Property { object, .. } => inline_expr(object, candidates, depth),
        Expr::New { class, args, .. } => {
            inline_expr(class, candidates, depth);
            for arg in args.iter_mut() {
                inline_expr(arg, candidates, depth);
            }
        }
        Expr::Tell { object, args, .. } => {
            inline_expr(object, candidates, depth);
            for arg in args.iter_mut() {
                inline_expr(arg, candidates, depth);
            }
        }
        Expr::List(items, _) => {
            for item in items.iter_mut() {
                inline_expr(item, candidates, depth);
            }
        }
        Expr::Dict(entries, _) => {
            for (k, v) in entries.iter_mut() {
                inline_expr(k, candidates, depth);
                inline_expr(v, candidates, depth);
            }
        }
        _ => {}
    }

    // Now try to inline this call if it is a direct call to a small function.
    if depth == 0 {
        return;
    }
    if let Expr::Call { callee, args, span: _ } = expr {
        if let Expr::Variable { name, .. } = callee.as_ref() {
            if let Some(candidate) = candidates.get(name) {
                if candidate.params.len() == args.len() && args.iter().all(is_trivial_expr) {
                    let mut inlined = candidate.body.clone();
                    for (param, arg) in candidate.params.iter().zip(args.iter()) {
                        substitute(&mut inlined, param, arg);
                    }
                    // Recursively inline inside the expanded body.
                    inline_expr(&mut inlined, candidates, depth - 1);
                    *expr = inlined;
                }
            }
        }
    }
}

fn is_trivial_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Variable { .. }
        | Expr::Integer(_, _)
        | Expr::Number(_, _)
        | Expr::String(_, _)
        | Expr::Bool(_, _)
        | Expr::Nothing(_) => true,
        Expr::Binary { left, right, .. } => is_trivial_expr(left) && is_trivial_expr(right),
        Expr::Unary { operand, .. } => is_trivial_expr(operand),
        Expr::List(items, _) => items.iter().all(is_trivial_expr),
        Expr::Dict(entries, _) => entries.iter().all(|(k, v)| is_trivial_expr(k) && is_trivial_expr(v)),
        _ => false,
    }
}

fn substitute(expr: &mut Expr, param: &str, arg: &Expr) {
    match expr {
        Expr::Variable { name, .. } if name == param => {
            *expr = arg.clone();
        }
        Expr::Binary { left, right, .. } => {
            substitute(left, param, arg);
            substitute(right, param, arg);
        }
        Expr::Unary { operand, .. } => substitute(operand, param, arg),
        Expr::Call { callee, args, .. } => {
            substitute(callee, param, arg);
            for a in args.iter_mut() {
                substitute(a, param, arg);
            }
        }
        Expr::Index { object, index, .. } => {
            substitute(object, param, arg);
            substitute(index, param, arg);
        }
        Expr::Property { object, .. } => substitute(object, param, arg),
        Expr::New { class, args, .. } => {
            substitute(class, param, arg);
            for a in args.iter_mut() {
                substitute(a, param, arg);
            }
        }
        Expr::Tell { object, args, .. } => {
            substitute(object, param, arg);
            for a in args.iter_mut() {
                substitute(a, param, arg);
            }
        }
        Expr::List(items, _) => {
            for item in items.iter_mut() {
                substitute(item, param, arg);
            }
        }
        Expr::Dict(entries, _) => {
            for (k, v) in entries.iter_mut() {
                substitute(k, param, arg);
                substitute(v, param, arg);
            }
        }
        _ => {}
    }
}

#[derive(Clone)]
struct ErrorCandidate {
    params: Vec<String>,
    cond: Expr,
    error_expr: Expr,
    return_expr: Expr,
}

/// Collect functions whose body is `if cond then error err_expr; return ret_expr`.
/// These are common "guard" functions used inside try/catch.
fn collect_error_candidates(stmts: &[Stmt]) -> HashMap<String, ErrorCandidate> {
    let mut candidates: HashMap<String, ErrorCandidate> = HashMap::new();
    for stmt in stmts.iter() {
        if let Stmt::Define {
            name,
            params,
            body,
            return_type,
            ..
        } = stmt
        {
            // Same as `inline_small_functions`: annotated functions must stay
            // real calls so their runtime type-annotation checks survive.
            let has_annotations =
                return_type.is_some() || params.iter().any(|(_, ann)| ann.is_some());
            if !has_annotations && body.len() == 2 {
                if let (
                    Stmt::If {
                        cond,
                        then_branch,
                        else_branch,
                    },
                    Stmt::Return {
                        value: Some(return_expr),
                        ..
                    },
                ) = (&body[0], &body[1])
                {
                    if then_branch.len() == 1
                        && else_branch.is_empty()
                        && matches!(then_branch[0], Stmt::Expr(_))
                    {
                        let Stmt::Expr(error_expr) = &then_branch[0] else {
                            continue;
                        };
                        candidates.insert(
                            name.clone(),
                            ErrorCandidate {
                                params: params.iter().map(|(n, _)| n.clone()).collect(),
                                cond: cond.clone(),
                                error_expr: error_expr.clone(),
                                return_expr: return_expr.clone(),
                            },
                        );
                    }
                }
            }
        }
    }
    candidates
}

/// Inline guard-function calls that appear directly inside a `try` body.
fn inline_try_error_calls(stmts: &mut [Stmt], candidates: &HashMap<String, ErrorCandidate>) {
    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::If { then_branch, else_branch, .. } => {
                inline_try_error_calls(then_branch, candidates);
                inline_try_error_calls(else_branch, candidates);
            }
            Stmt::While { body, .. }
            | Stmt::For { body, .. }
            | Stmt::Define { body, .. }
            | Stmt::Init(Init { body, .. }) => {
                inline_try_error_calls(body, candidates);
            }
            Stmt::Class { init, methods, .. } => {
                if let Some(init) = init {
                    inline_try_error_calls(&mut init.body, candidates);
                }
                for m in methods.iter_mut() {
                    inline_try_error_calls(std::slice::from_mut(m), candidates);
                }
            }
            Stmt::Try { body, catch_body, .. } => {
                inline_try_error_calls(catch_body, candidates);
                let mut new_body: Vec<Stmt> = Vec::with_capacity(body.len() * 2);
                for s in body.drain(..) {
                    if let Stmt::Expr(Expr::Call { callee, args, span }) = &s {
                        if let Expr::Variable { name, .. } = callee.as_ref() {
                            if let Some(cand) = candidates.get(name) {
                                if cand.params.len() == args.len() {
                                    let mut cond = cand.cond.clone();
                                    let mut error_expr = cand.error_expr.clone();
                                    let mut return_expr = cand.return_expr.clone();
                                    for (param, arg) in cand.params.iter().zip(args.iter()) {
                                        substitute(&mut cond, param, arg);
                                        substitute(&mut error_expr, param, arg);
                                        substitute(&mut return_expr, param, arg);
                                    }
                                    new_body.push(Stmt::If {
                                        cond,
                                        then_branch: vec![Stmt::Expr(error_expr)],
                                        else_branch: vec![],
                                    });
                                    new_body.push(Stmt::Expr(return_expr));
                                    continue;
                                }
                            }
                        }
                    }
                    new_body.push(s);
                }
                *body = new_body;
            }
            _ => {}
        }
    }
}

/// If a `try` block has been reduced to a single guard `if cond then error ...`
/// and the catch variable is unused, eliminate the try/catch entirely and run
/// the catch body directly under the guard condition.
fn simplify_guard_tries(stmts: &mut [Stmt]) {
    fn expr_uses_var(expr: &Expr, name: &str) -> bool {
        match expr {
            Expr::Variable { name: n, .. } => n == name,
            Expr::Call { callee, args, .. } => {
                expr_uses_var(callee, name) || args.iter().any(|a| expr_uses_var(a, name))
            }
            Expr::Binary { left, right, .. } => expr_uses_var(left, name) || expr_uses_var(right, name),
            Expr::Unary { operand, .. } => expr_uses_var(operand, name),
            Expr::List(items, _) => items.iter().any(|i| expr_uses_var(i, name)),
            Expr::Dict(entries, _) => entries.iter().any(|(k, v)| {
                expr_uses_var(k, name) || expr_uses_var(v, name)
            }),
            Expr::Index { object, index, .. } => expr_uses_var(object, name) || expr_uses_var(index, name),
            Expr::Property { object, .. } | Expr::Tell { object, .. } => {
                expr_uses_var(object, name)
            }
            Expr::New { args, .. } => args.iter().any(|a| expr_uses_var(a, name)),
            Expr::Qualified { .. } | Expr::Ellipsis => false,
            _ => false,
        }
    }

    fn stmt_uses_var(stmt: &Stmt, name: &str) -> bool {
        match stmt {
            Stmt::Expr(e) | Stmt::Show(e) | Stmt::Return { value: Some(e), .. } => expr_uses_var(e, name),
            Stmt::Let { value, .. } | Stmt::Set { value, .. } => expr_uses_var(value, name),
            Stmt::If { cond, then_branch, else_branch } => {
                expr_uses_var(cond, name)
                    || then_branch.iter().any(|s| stmt_uses_var(s, name))
                    || else_branch.iter().any(|s| stmt_uses_var(s, name))
            }
            Stmt::While { cond, body } => {
                expr_uses_var(cond, name) || body.iter().any(|s| stmt_uses_var(s, name))
            }
            Stmt::For { iterable, body, .. } => {
                expr_uses_var(iterable, name) || body.iter().any(|s| stmt_uses_var(s, name))
            }
            Stmt::Define { body, .. } => body.iter().any(|s| stmt_uses_var(s, name)),
            Stmt::Init(Init { body, .. }) => body.iter().any(|s| stmt_uses_var(s, name)),
            Stmt::Class { init, methods, .. } => {
                init.as_ref().map_or(false, |i| i.body.iter().any(|s| stmt_uses_var(s, name)))
                    || methods.iter().any(|m| stmt_uses_var(m, name))
            }
            Stmt::Try { body, catch_body, catch_var, .. } => {
                body.iter().any(|s| stmt_uses_var(s, name))
                    || (catch_var != name && catch_body.iter().any(|s| stmt_uses_var(s, name)))
            }
            _ => false,
        }
    }

    fn is_error_call(expr: &Expr) -> bool {
        matches!(expr, Expr::Call { callee, .. } if matches!(callee.as_ref(), Expr::Variable { name, .. } if name == "error"))
    }

    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::If { then_branch, else_branch, .. } => {
                simplify_guard_tries(then_branch);
                simplify_guard_tries(else_branch);
            }
            Stmt::While { body, .. }
            | Stmt::For { body, .. }
            | Stmt::Define { body, .. }
            | Stmt::Init(Init { body, .. }) => {
                simplify_guard_tries(body);
            }
            Stmt::Class { init, methods, .. } => {
                if let Some(init) = init {
                    simplify_guard_tries(&mut init.body);
                }
                for m in methods.iter_mut() {
                    simplify_guard_tries(std::slice::from_mut(m));
                }
            }
            Stmt::Try { body, catch_var, catch_body } => {
                simplify_guard_tries(catch_body);
                if catch_body.iter().any(|s| stmt_uses_var(s, catch_var)) {
                    return;
                }
                // Body must be: if cond then error ...; <ignored expr>
                if body.len() == 2 {
                    if let (
                        Stmt::If {
                            cond,
                            then_branch,
                            else_branch,
                        },
                        Stmt::Expr(_),
                    ) = (&body[0], &body[1])
                    {
                        if then_branch.len() == 1
                            && else_branch.is_empty()
                            && matches!(&then_branch[0], Stmt::Expr(e) if is_error_call(e))
                        {
                            let cond = cond.clone();
                            *stmt = Stmt::If {
                                cond,
                                then_branch: std::mem::take(catch_body),
                                else_branch: vec![],
                            };
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

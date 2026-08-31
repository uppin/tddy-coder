//! Per-function cyclomatic complexity for Rust sources (`syn` walker).

use serde::Serialize;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{BinOp, Expr, Stmt};

use crate::error::Result;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FunctionComplexity {
    pub name: String,
    pub line: u32,
    pub complexity: u32,
}

struct ComplexityVisitor {
    functions: Vec<FunctionComplexity>,
}

impl ComplexityVisitor {
    fn measure_function(name: String, line: u32, body: &[Stmt]) -> FunctionComplexity {
        let complexity = 1 + body.iter().map(walk_stmt).sum::<u32>();
        FunctionComplexity {
            name,
            line,
            complexity,
        }
    }
}

fn count_decisions_expr(expr: &Expr) -> u32 {
    match expr {
        Expr::If(_) | Expr::While(_) | Expr::ForLoop(_) | Expr::Loop(_) | Expr::Match(_) => 1,
        Expr::Let(_) => 1,
        Expr::Binary(binary) => match binary.op {
            BinOp::And(_) | BinOp::Or(_) => 1,
            _ => 0,
        },
        Expr::Try(_) => 1,
        _ => 0,
    }
}

fn walk_expr(expr: &Expr) -> u32 {
    let decision = count_decisions_expr(expr);
    let nested = match expr {
        Expr::If(e) => {
            walk_expr(&e.cond)
                + walk_block(&e.then_branch)
                + e.else_branch
                    .as_ref()
                    .map(|(_, else_branch)| walk_else(else_branch))
                    .unwrap_or(0)
        }
        Expr::While(e) => walk_expr(&e.cond) + walk_block(&e.body),
        Expr::ForLoop(e) => walk_block(&e.body),
        Expr::Loop(e) => walk_block(&e.body),
        Expr::Match(e) => e
            .arms
            .iter()
            .map(|arm| walk_expr_body(&arm.body))
            .sum::<u32>(),
        Expr::Binary(e) => walk_expr(&e.left) + walk_expr(&e.right),
        Expr::Let(e) => walk_expr(&e.expr),
        Expr::Try(e) => walk_expr(&e.expr),
        Expr::Unary(e) => walk_expr(&e.expr),
        Expr::Cast(e) => walk_expr(&e.expr),
        Expr::Await(e) => walk_expr(&e.base),
        Expr::Assign(e) => walk_expr(&e.left) + walk_expr(&e.right),
        Expr::Reference(e) => walk_expr(&e.expr),
        Expr::Break(e) => e.expr.as_ref().map(|expr| walk_expr(expr)).unwrap_or(0),
        Expr::Continue(_) => 0,
        Expr::Return(e) => e.expr.as_ref().map(|expr| walk_expr(expr)).unwrap_or(0),
        Expr::Yield(e) => e.expr.as_ref().map(|expr| walk_expr(expr)).unwrap_or(0),
        Expr::Closure(_) => 0,
        Expr::Block(e) => walk_block(&e.block),
        Expr::Call(e) => walk_expr(&e.func) + e.args.iter().map(walk_expr).sum::<u32>(),
        Expr::MethodCall(e) => walk_expr(&e.receiver) + e.args.iter().map(walk_expr).sum::<u32>(),
        Expr::Index(e) => walk_expr(&e.expr) + walk_expr(&e.index),
        Expr::Field(e) => walk_expr(&e.base),
        Expr::Tuple(e) => e.elems.iter().map(walk_expr).sum(),
        Expr::Array(e) => e.elems.iter().map(walk_expr).sum(),
        Expr::Paren(e) => walk_expr(&e.expr),
        Expr::Group(e) => walk_expr(&e.expr),
        _ => 0,
    };
    decision + nested
}

fn walk_stmt(stmt: &Stmt) -> u32 {
    match stmt {
        Stmt::Item(_) => 0,
        Stmt::Local(local) => local
            .init
            .as_ref()
            .map(|init| walk_expr(&init.expr))
            .unwrap_or(0),
        Stmt::Expr(expr, _) => walk_expr(expr),
        _ => 0,
    }
}

fn walk_block(block: &syn::Block) -> u32 {
    block.stmts.iter().map(walk_stmt).sum()
}

fn walk_expr_body(expr: &Expr) -> u32 {
    match expr {
        Expr::Block(block) => walk_block(&block.block),
        other => walk_expr(other),
    }
}

fn walk_else(expr: &Expr) -> u32 {
    match expr {
        Expr::Block(block) => walk_block(&block.block),
        Expr::If(expr_if) => {
            1 + walk_expr(&expr_if.cond)
                + walk_block(&expr_if.then_branch)
                + expr_if
                    .else_branch
                    .as_ref()
                    .map(|(_, else_branch)| walk_else(else_branch))
                    .unwrap_or(0)
        }
        other => walk_expr(other),
    }
}

impl<'ast> Visit<'ast> for ComplexityVisitor {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let line = node.sig.ident.span().start().line as u32;
        let name = node.sig.ident.to_string();
        self.functions
            .push(Self::measure_function(name, line, &node.block.stmts));
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let line = node.sig.ident.span().start().line as u32;
        let name = node.sig.ident.to_string();
        self.functions
            .push(Self::measure_function(name, line, &node.block.stmts));
        syn::visit::visit_impl_item_fn(self, node);
    }

    fn visit_expr_closure(&mut self, node: &'ast syn::ExprClosure) {
        let line = node.span().start().line as u32;
        let name = format!("<closure@{}>", line);
        if let Expr::Block(block) = &*node.body {
            self.functions
                .push(Self::measure_function(name, line, &block.block.stmts));
        }
        syn::visit::visit_expr_closure(self, node);
    }
}

/// Measure every function in a Rust source file.
pub fn file_complexity(source: &str) -> Result<Vec<FunctionComplexity>> {
    let file = syn::parse_file(source)?;
    let mut visitor = ComplexityVisitor {
        functions: Vec::new(),
    };
    visitor.visit_file(&file);
    Ok(visitor.functions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_decision_points_in_a_function_body() {
        // Given
        let source = r#"
pub fn classify(x: i32) -> &'static str {
    if x < 0 {
        "neg"
    } else if x == 0 {
        "zero"
    } else {
        "pos"
    }
}
"#;

        // When
        let measured = file_complexity(source).expect("parse");

        // Then
        let classify = measured
            .iter()
            .find(|f| f.name == "classify")
            .expect("classify measured");
        assert_eq!(classify.line, 2);
        assert_eq!(classify.complexity, 3);
    }
}

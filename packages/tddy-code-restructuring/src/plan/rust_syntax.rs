//! The Rust syntax a plan may carry, and the only Rust syntax: one type or one expression.
//!
//! A plan holds intents, never code — `text`, `code` and `content` are refused outright. A signature
//! change cannot be stated without naming a type, though, and a call-site repair cannot be stated
//! without naming an argument, so `type` and `expr` are the two exceptions. Each is parsed with
//! `syn` as **exactly one** [`syn::Type`] or [`syn::Expr`] and refused otherwise: neither can carry a
//! statement or an item, which is what keeps the exception from becoming a way to hand the engine a
//! snippet.
//!
//! `order` is not syntax, but whether it is a permutation of what it reorders is decided here too,
//! once the backend has read the declaration or the call it applies to.

use super::malformed;
use crate::plan::OrderKey;
use crate::Result;

/// `text` as exactly one Rust type, or a malformed-plan refusal naming it.
///
/// Refuses anything `syn` does not parse as one whole [`syn::Type`] — two types, a type followed by
/// anything else, or an expression.
pub fn one_type(text: &str) -> Result<syn::Type> {
    syn::parse_str::<syn::Type>(text).map_err(|_| {
        malformed(format!(
            "`type` must be exactly one Rust type, and `{text}` is not"
        ))
    })
}

/// `text` as exactly one Rust expression carrying no statement, or a malformed-plan refusal
/// naming it.
///
/// Refuses anything `syn` does not parse as one whole [`syn::Expr`], and an expression that parses
/// but carries a statement anywhere inside it — a block, a closure body, an `if` arm — because a
/// statement is code the engine should have produced, not an argument.
pub fn one_expr(text: &str) -> Result<syn::Expr> {
    let expr = syn::parse_str::<syn::Expr>(text).map_err(|_| {
        malformed(format!(
            "`expr` must be exactly one Rust expression, and `{text}` is not"
        ))
    })?;

    let mut statements = Statements::default();
    syn::visit::Visit::visit_expr(&mut statements, &expr);
    if statements.found {
        return Err(malformed(format!(
            "`expr` may carry no statement, and `{text}` carries one"
        )));
    }
    Ok(expr)
}

/// Whether the expression visited carries a [`syn::Stmt`] anywhere inside it.
#[derive(Default)]
struct Statements {
    found: bool,
}

impl<'ast> syn::visit::Visit<'ast> for Statements {
    fn visit_stmt(&mut self, _: &'ast syn::Stmt) {
        self.found = true;
    }
}

/// `text` as one callee: a path (`a::b::f`, `<T as Tr>::f`) or a field access whose chain holds only
/// paths, fields, tuple indexes, calls, method calls, `(…)`, `&`, `*`, `?`, `.await` and indexing,
/// or a malformed-plan refusal naming the text.
///
/// The outer expression is never a call: the arguments stay with the call being re-pointed, so
/// `f(x)` and `a.b()` are refused, and so is every statement-bearing or operator expression.
pub fn one_callee(text: &str) -> Result<syn::Expr> {
    let refused = |why: &str| {
        malformed(format!(
            "`callee` must be one path or field/method chain, `self.peer.f` or `a::b::f`, and \
             `{text}` is not: {why}"
        ))
    };
    let expr = syn::parse_str::<syn::Expr>(text)
        .map_err(|_| refused("it does not parse as exactly one expression"))?;
    if !matches!(expr, syn::Expr::Path(_) | syn::Expr::Field(_)) {
        return Err(refused(
            "the outer expression is a call or an operator, and the arguments stay with the call",
        ));
    }
    let mut chain = Chain::default();
    syn::visit::Visit::visit_expr(&mut chain, &expr);
    match chain.refused {
        Some(kind) => Err(refused(&format!("it holds {kind}"))),
        None => Ok(expr),
    }
}

/// What a callee's chain may not hold: a statement, or an expression that is not a chain element.
#[derive(Default)]
struct Chain {
    refused: Option<&'static str>,
}

impl<'ast> syn::visit::Visit<'ast> for Chain {
    fn visit_stmt(&mut self, _: &'ast syn::Stmt) {
        self.refused.get_or_insert("a statement");
    }

    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        use syn::Expr::*;
        let kind = match expr {
            Block(_) | Unsafe(_) | Const(_) | Async(_) | TryBlock(_) => Some("a block"),
            Closure(_) => Some("a closure"),
            If(_) | Match(_) | While(_) | Loop(_) | ForLoop(_) | Let(_) => Some("control flow"),
            Macro(_) => Some("a macro"),
            Binary(_) | Assign(_) | Range(_) | Cast(_) => Some("an operator expression"),
            Return(_) | Break(_) | Continue(_) | Yield(_) => Some("a jump"),
            Unary(unary) if !matches!(unary.op, syn::UnOp::Deref(_)) => Some("a unary operator"),
            _ => None,
        };
        match kind {
            Some(kind) => {
                self.refused.get_or_insert(kind);
            }
            None => syn::visit::visit_expr(self, expr),
        }
    }
}

/// The hops a bulk `callee` template inserts after a receiver: `$receiver.peer.slot` for the method
/// `slot` is `[".peer"]`, `$receiver.agent_roster().slot` is `[".agent_roster()"]`.
///
/// `$receiver` must occur once, as the leftmost segment, followed by at least one hop, and the last
/// segment must be the method's own name: a rename is `rename_symbol`'s.
pub fn one_receiver_template(text: &str, method: &str) -> Result<Vec<String>> {
    const SENTINEL: &str = "__receiver__";
    let refused = |why: &str| {
        malformed(format!(
            "`callee` of a bulk `repoint_call` is `$receiver<hops>.{method}`, and `{text}` is not: \
             {why}"
        ))
    };
    if text.matches("$receiver").count() != 1 || !text.starts_with("$receiver") {
        return Err(refused(
            "`$receiver` must occur once, as the leftmost segment",
        ));
    }
    let rest = &text["$receiver".len()..];
    let tail = format!(".{method}");
    let hops_text = rest
        .strip_suffix(&tail)
        .filter(|hops| hops.starts_with('.'))
        .ok_or_else(|| refused("it must add at least one hop and end in the method's own name"))?;
    one_callee(&format!("{SENTINEL}{rest}")).map_err(|_| refused("it is not one chain"))?;
    Ok(split_hops(hops_text))
}

/// `.a.b().c[0]` as `[".a", ".b()", ".c[0]"]`: the pieces between the dots outside any bracket.
fn split_hops(hops: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut depth = 0usize;
    let mut from = 0usize;
    for (at, character) in hops.char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            '.' if depth == 0 && at > from => {
                found.push(hops[from..at].trim().to_string());
                from = at;
            }
            _ => {}
        }
    }
    found.push(hops[from..].trim().to_string());
    found
}

/// Where each entry of `current` goes under `order`: the index into `current` of each new position.
///
/// `order` must name every entry of `current` exactly once. A missing one or a repeated one is
/// refused naming it — reordering is never allowed to drop or duplicate a parameter or an argument.
pub fn permutation(current: &[OrderKey], order: &[OrderKey]) -> Result<Vec<usize>> {
    let mut taken = vec![false; current.len()];
    let mut moved = Vec::with_capacity(order.len());
    for key in order {
        let index = current
            .iter()
            .position(|known| known == key)
            .ok_or_else(|| malformed(format!("`order` names {key}, which is not there")))?;
        if std::mem::replace(&mut taken[index], true) {
            return Err(malformed(format!("`order` names {key} twice")));
        }
        moved.push(index);
    }
    if let Some(missing) = taken.iter().position(|taken| !taken) {
        return Err(malformed(format!(
            "`order` must name every entry once: {} is missing",
            current[missing]
        )));
    }
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// `total(price, quantity, discount)`'s parameters, in declaration order.
    fn the_parameters_of_total() -> Vec<OrderKey> {
        names(&["price", "quantity", "discount"])
    }

    fn names(names: &[&str]) -> Vec<OrderKey> {
        names
            .iter()
            .map(|name| OrderKey::Name((*name).to_string()))
            .collect()
    }

    /// The refusal `permutation` gives for `order` over [`the_parameters_of_total`], as text.
    fn the_refusal_reordering_total_by(order: &[&str]) -> String {
        permutation(&the_parameters_of_total(), &names(order))
            .expect_err("the order is not a permutation of the parameters")
            .to_string()
    }

    #[test]
    fn order_must_name_every_parameter_once() {
        // Given `total(price, quantity, discount)`

        // When it is reordered leaving `quantity` out, and naming `price` twice
        let leaving_one_out = the_refusal_reordering_total_by(&["discount", "price"]);
        let naming_one_twice =
            the_refusal_reordering_total_by(&["discount", "price", "price", "quantity"]);

        // Then each is refused naming the parameter at fault
        assert_eq!(
            (leaving_one_out, naming_one_twice),
            (
                "plan is malformed: `order` must name every entry once: `quantity` is missing"
                    .to_string(),
                "plan is malformed: `order` names `price` twice".to_string(),
            )
        );
    }

    #[test]
    fn an_order_naming_every_parameter_once_maps_each_new_position_to_its_old_one() {
        // Given `total(price, quantity, discount)`

        // When it is reordered to `(discount, price, quantity)`
        let moved = permutation(
            &the_parameters_of_total(),
            &names(&["discount", "price", "quantity"]),
        );

        // Then the new first parameter was the third, and so on
        assert_eq!(moved.map_err(|error| error.to_string()), Ok(vec![2, 0, 1]));
    }
}

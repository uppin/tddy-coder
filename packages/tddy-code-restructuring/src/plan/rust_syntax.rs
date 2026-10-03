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

use crate::plan::OrderKey;
use crate::Result;

/// `text` as exactly one Rust type, or a malformed-plan refusal naming it.
///
/// Refuses anything `syn` does not parse as one whole [`syn::Type`] — two types, a type followed by
/// anything else, or an expression.
pub fn one_type(text: &str) -> Result<syn::Type> {
    // TODO(signature-rewrites): implement — `syn::parse_str::<syn::Type>(text)`, refusing as
    // malformed with "`type` must be exactly one Rust type, and `<text>` is not".
    let _ = text;
    todo!("TODO(signature-rewrites): parse `type` as exactly one Rust type")
}

/// `text` as exactly one Rust expression carrying no statement, or a malformed-plan refusal
/// naming it.
///
/// Refuses anything `syn` does not parse as one whole [`syn::Expr`], and an expression that parses
/// but carries a statement anywhere inside it — a block, a closure body, an `if` arm — because a
/// statement is code the engine should have produced, not an argument.
pub fn one_expr(text: &str) -> Result<syn::Expr> {
    // TODO(signature-rewrites): implement — `syn::parse_str::<syn::Expr>(text)`, refusing as
    // malformed with "`expr` must be exactly one Rust expression, and `<text>` is not"; then walk it
    // with `syn::visit::Visit` and refuse any `Stmt` with "`expr` may carry no statement, and
    // `<text>` carries one".
    let _ = text;
    todo!("TODO(signature-rewrites): parse `expr` as exactly one Rust expression")
}

/// Where each entry of `current` goes under `order`: the index into `current` of each new position.
///
/// `order` must name every entry of `current` exactly once. A missing one or a repeated one is
/// refused naming it — reordering is never allowed to drop or duplicate a parameter or an argument.
pub fn permutation(current: &[OrderKey], order: &[OrderKey]) -> Result<Vec<usize>> {
    // TODO(signature-rewrites): implement — refuse an entry of `order` that is not in `current`
    // ("`order` names <key>, which is not there"), one named twice ("`order` names <key> twice") and
    // one of `current` it leaves out ("`order` must name every entry once: <key> is missing").
    let _ = (current, order);
    todo!("TODO(signature-rewrites): check `order` is a permutation of what it reorders")
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

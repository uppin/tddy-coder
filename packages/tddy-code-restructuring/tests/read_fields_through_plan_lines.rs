//! The plan line of `read_fields_through`, and the refusals a plain `check` makes from the text alone.
//!
//! A method of a type that cannot leave its crate can only have its body moved once the body stops
//! naming `self`. `read_fields_through` inserts `let <name> = <expr>;` before a range and rebinds the
//! `self` the range reads to `<name>`. Everything here is answered by the plan and one file's text,
//! before any index exists: the parse-time refusals (RP1-RP4, `plan is malformed:`) and the lexical
//! ones (RS1-RS4) a static check reports for a range anchor.

mod harness;
mod same_crate;

use std::collections::BTreeSet;

use harness::AFixtureWorkspace;
use same_crate::{an_app_holding, what_a_static_check_finds_in};
use tddy_code_restructuring::{Plan, RefactorKind, RefactorOp};

const A_HEADER: &str = "{\"v\":1,\"snapshot\":{}}";

const A_RANGE: &str = r#"{"kind":"range","file":"src/host.rs","start":{"line":9,"col":9},"end":{"line":10,"col":31}}"#;
const A_SYMBOL: &str = r#"{"kind":"symbol","file":"src/host.rs","path":"Host"}"#;
const ITEMS: &str = r#"{"kind":"items","file":"src/host.rs","items":["app::host::Host::total"],"fingerprints":["sha256:a"]}"#;
const A_RANGELESS_ITEM: &str = r#"{"kind":"item","item":"app::host::Host::total","file":"src/host.rs","fingerprint":"sha256:a"}"#;

const THE_BINDING: &str = ",\"name\":\"state\",\"expr\":\"self.state()\"";

/// A plan of one line: `op`, anchored on `anchor`, with the `extra` fields written after them.
fn a_plan_of_one(op: &str, anchor: &str, extra: &str) -> String {
    format!("{A_HEADER}\n{{\"op\":\"{op}\",\"anchor\":{anchor}{extra}}}\n")
}

fn a_rebind_line(anchor: &str, extra: &str) -> String {
    a_plan_of_one("read_fields_through", anchor, extra)
}

/// What reading the plan refuses with, or fails saying it did not refuse.
fn refusal_of(plan: &str) -> String {
    match Plan::parse(plan) {
        Err(refusal) => refusal.to_string(),
        Ok(_) => panic!("the plan was expected to be refused, and parsed:\n{plan}"),
    }
}

/// The host of the static findings. Line 8 is `total`'s first statement, which reads no `self`;
/// lines 9-11 read two fields, call a method and use the type keyword.
const A_HOST: &str = concat!(
    "pub struct Host {\n",
    "    rosters: Vec<u32>,\n",
    "    config: u32,\n",
    "}\n",
    "\n",
    "impl Host {\n",
    "    pub fn total(&self, floor: u32) -> u32 {\n",
    "        let base = floor + 1;\n",
    "        let above = self.rosters.len() as u32 + base;\n",
    "        above + self.config + self.extra()\n",
    "    }\n",
    "\n",
    "    fn extra(&self) -> u32 {\n",
    "        1\n",
    "    }\n",
    "}\n",
);

fn an_app_whose_host_reads_its_fields() -> AFixtureWorkspace {
    an_app_holding(&[("src/lib.rs", "pub mod host;\n"), ("src/host.rs", A_HOST)])
}

/// A `read_fields_through` over `start..end` (one-based `(line, col)`) of `src/host.rs`, binding
/// `name` to `expr`.
fn a_rebind_op(start: (u32, u32), end: (u32, u32), name: &str, expr: &str) -> RefactorOp {
    serde_json::from_value(serde_json::json!({
        "op": "read_fields_through",
        "anchor": {
            "kind": "range",
            "file": "src/host.rs",
            "start": { "line": start.0, "col": start.1 },
            "end": { "line": end.0, "col": end.1 },
        },
        "name": name,
        "expr": expr,
    }))
    .expect("a `read_fields_through` operation deserializes")
}

#[test]
fn a_read_fields_through_line_parses_and_reads_back_without_its_defaults() {
    // Given a rebind line carrying only what it needs
    let plan = a_rebind_line(A_RANGE, THE_BINDING);

    // When it is parsed and written back
    let parsed = Plan::parse(&plan).expect("the plan parses");
    let written = parsed.to_jsonl();
    let line: serde_json::Value =
        serde_json::from_str(written.lines().nth(1).expect("the operation's line"))
            .expect("the line is JSON");
    let fields: BTreeSet<&str> = line
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();

    // Then it is the rebind, naming its binding and its value, and no empty field is written
    assert_eq!(
        (
            parsed.ops[0].op,
            parsed.ops[0].name.as_deref(),
            parsed.ops[0].expr.as_deref()
        ),
        (
            RefactorKind::ReadFieldsThrough,
            Some("state"),
            Some("self.state()")
        )
    );
    assert_eq!(fields, BTreeSet::from(["op", "anchor", "name", "expr"]));
}

#[test]
fn a_read_fields_through_without_name_or_expr_is_malformed() {
    // Given a rebind with no binding, and another with no value
    let without_name = a_rebind_line(A_RANGE, ",\"expr\":\"self.state()\"");
    let without_expr = a_rebind_line(A_RANGE, ",\"name\":\"state\"");

    // When each is read
    let refusals = [refusal_of(&without_name), refusal_of(&without_expr)];

    // Then each is malformed and names what it lacks
    assert!(
        refusals[0].starts_with("plan is malformed:")
            && refusals[0].contains(
                "`read_fields_through` needs `name`: the binding the range reads `self` through"
            ),
        "unexpected refusal: {}",
        refusals[0]
    );
    assert!(
        refusals[1].starts_with("plan is malformed:")
            && refusals[1]
                .contains("`read_fields_through` needs `expr`: the value `state` is bound to"),
        "unexpected refusal: {}",
        refusals[1]
    );
}

#[test]
fn a_binding_that_is_self_a_keyword_or_not_one_identifier_is_malformed() {
    // Given bindings that cannot be a new local's name
    let names = ["self", "match", "two words", "a.b"];

    // When each is read
    let refusals: Vec<String> = names
        .iter()
        .map(|name| {
            refusal_of(&a_rebind_line(
                A_RANGE,
                &format!(
                    ",\"name\":{},\"expr\":\"self.state()\"",
                    serde_json::json!(name)
                ),
            ))
        })
        .collect();

    // Then each is refused naming the binding
    let expected: Vec<String> = names
        .iter()
        .map(|name| format!("`name` must be one identifier other than `self`; `{name}` is not"))
        .collect();
    assert!(
        refusals
            .iter()
            .zip(&expected)
            .all(
                |(refusal, wanted)| refusal.starts_with("plan is malformed:")
                    && refusal.contains(wanted.as_str())
            ),
        "unexpected refusals: {refusals:?}"
    );
}

#[test]
fn a_symbol_an_items_or_a_rangeless_item_anchor_names_no_statements_to_rebind() {
    // Given a rebind anchored on a symbol, on items, and on an item with no relative range
    let anchors = [A_SYMBOL, ITEMS, A_RANGELESS_ITEM];

    // When each is read
    let refusals: Vec<String> = anchors
        .iter()
        .map(|anchor| refusal_of(&a_rebind_line(anchor, THE_BINDING)))
        .collect();

    // Then each says the operation anchors on a range
    assert!(
        refusals.iter().all(|refusal| refusal.starts_with("plan is malformed:")
            && refusal.contains(
                "`read_fields_through` anchors on a range (a `range`, or an `item` with a relative range)"
            )),
        "unexpected refusals: {refusals:?}"
    );
}

#[test]
fn every_field_read_fields_through_does_not_define_is_refused_naming_it() {
    // Given a rebind carrying, one at a time, every field it has no use for
    let extras = [
        ("to", ",\"to\":\"app::other\""),
        ("to_type", ",\"to_type\":\"app::host::Host\""),
        ("variant", ",\"variant\":\"leave_delegator\""),
        ("callee", ",\"callee\":\"self.other\""),
        ("type", ",\"type\":\"u32\""),
        ("order", ",\"order\":[\"a\"]"),
        ("reexport", ",\"reexport\":\"glob\""),
        (
            "also",
            ",\"also\":[{\"kind\":\"symbol\",\"file\":\"src/a.rs\",\"path\":\"a\"}]",
        ),
        ("to_file", ",\"to_file\":true"),
        ("canonical_paths", ",\"canonical_paths\":true"),
        ("with_private_deps", ",\"with_private_deps\":true"),
    ];

    // When each is read
    let refusals: Vec<(&str, String)> = extras
        .iter()
        .map(|(field, extra)| {
            (
                *field,
                refusal_of(&a_rebind_line(A_RANGE, &format!("{THE_BINDING}{extra}"))),
            )
        })
        .collect();

    // Then each is malformed and names its own field
    let unnamed: Vec<&(&str, String)> = refusals
        .iter()
        .filter(|(field, refusal)| {
            !(refusal.starts_with("plan is malformed:") && refusal.contains(&format!("`{field}`")))
        })
        .collect();
    assert!(
        unnamed.is_empty(),
        "these refusals do not name their field: {unnamed:?}"
    );
}

#[tokio::test]
async fn a_static_check_refuses_a_mid_statement_start_a_range_without_self_a_method_call_in_field_mode_and_a_shadowing_binding(
) {
    // Given a host whose `total` reads two fields, calls a method and binds `base`
    let workspace = an_app_whose_host_reads_its_fields();

    // When a static check examines four rebinds: one starting mid-statement, one over a statement
    // that names no `self`, one in field mode over a method call, and one binding `base`
    let mid_statement = what_a_static_check_finds_in(
        &workspace,
        a_rebind_op((9, 21), (10, 43), "state", "self.state()"),
    )
    .await;
    let without_self = what_a_static_check_finds_in(
        &workspace,
        a_rebind_op((8, 9), (8, 30), "state", "self.state()"),
    )
    .await;
    let a_method_call = what_a_static_check_finds_in(
        &workspace,
        a_rebind_op((9, 9), (10, 43), "state", "self.state()"),
    )
    .await;
    let shadowing =
        what_a_static_check_finds_in(&workspace, a_rebind_op((9, 9), (10, 43), "base", "self"))
            .await;

    // Then each is refused naming its line, and self mode admits the method call
    assert!(
        mid_statement.iter().any(|finding| finding
            .contains("the range starts at line 9 col 21, which is not the start of a statement")),
        "{mid_statement:?}"
    );
    assert!(
        without_self.iter().any(
            |finding| finding.contains("the range names no `self`: there is nothing to rebind")
        ),
        "{without_self:?}"
    );
    assert!(
        a_method_call
            .iter()
            .any(|finding| finding.contains("the range calls `self.extra(…)` at line 10")),
        "{a_method_call:?}"
    );
    assert_eq!(
        shadowing,
        vec![
            "`base` is already written at line 8 of this function: the new binding would shadow it"
                .to_string()
        ]
    );
}

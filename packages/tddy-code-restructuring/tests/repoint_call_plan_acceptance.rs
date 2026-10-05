//! The plan line of `repoint_call`, read without a language server.
//!
//! One operation, two forms, read from the anchor's shape: an item anchor with a relative range over
//! one call replaces that call's callee (the **single** form); an item anchor with no range, on a
//! method, takes a `$receiver<hops>.<method>` template and inserts the hops after the receiver of every
//! call of that method (the **bulk** form). Everything here is answered by the text of the plan and
//! of one file, so it is answered in milliseconds and before any index exists: the parse-time
//! refusals (`plan is malformed:`, fixed by editing the plan) and the findings a plain `check` makes
//! over a range anchor, the only anchor a static pass can examine.

mod harness;
mod same_crate;

use std::collections::BTreeSet;

use same_crate::{an_app_holding, what_a_static_check_finds_in};
use tddy_code_restructuring::{Plan, RefactorKind, RefactorOp};

const A_HEADER: &str = "{\"v\":1,\"snapshot\":{}}";

const A_CALL_IN_ROSTER_WORK: &str = r#"{"kind":"item","item":"app::host::Host::roster_work","file":"src/host.rs","start":{"line":3,"col":9},"end":{"line":3,"col":38},"fingerprint":"sha256:a"}"#;
const A_METHOD: &str =
    r#"{"kind":"item","item":"app::host::Host::m","file":"src/host.rs","fingerprint":"sha256:a"}"#;
const A_SYMBOL: &str = r#"{"kind":"symbol","file":"src/host.rs","path":"Host"}"#;
const ITEMS: &str = r#"{"kind":"items","file":"src/host.rs","items":["app::host::Host::m","app::host::Host::n"],"fingerprints":["sha256:a","sha256:b"]}"#;

/// A plan of one line: `op`, anchored on `anchor`, with the `extra` fields written after them.
fn a_plan_of_one(op: &str, anchor: &str, extra: &str) -> String {
    format!("{A_HEADER}\n{{\"op\":\"{op}\",\"anchor\":{anchor}{extra}}}\n")
}

/// A `repoint_call` over `anchor` whose `callee` is `callee`, with `extra` fields after it.
fn a_repoint_to(anchor: &str, callee: &str, extra: &str) -> String {
    a_plan_of_one(
        "repoint_call",
        anchor,
        &format!(",\"callee\":{}{extra}", serde_json::json!(callee)),
    )
}

/// What reading the plan refuses with, or fails saying it did not refuse.
fn refusal_of(plan: &str) -> String {
    match Plan::parse(plan) {
        Err(refusal) => refusal.to_string(),
        Ok(_) => panic!("the plan was expected to be refused, and parsed:\n{plan}"),
    }
}

fn the_only_operation_of(plan: &str) -> RefactorOp {
    let mut parsed = Plan::parse(plan).expect("the plan parses");
    assert_eq!(parsed.ops.len(), 1, "the plan holds one operation");
    parsed.ops.remove(0)
}

#[test]
fn a_repoint_call_with_a_callee_over_a_call_is_a_plan() {
    // Given a single-form line carrying only what it needs
    let plan = a_repoint_to(
        A_CALL_IN_ROSTER_WORK,
        "self.peer_routing.common_room_slot",
        "",
    );

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

    // Then it is the re-point operation, naming the new callee, and no empty field is written
    assert_eq!(
        (parsed.ops[0].op, parsed.ops[0].callee.as_deref()),
        (
            RefactorKind::RepointCall,
            Some("self.peer_routing.common_room_slot")
        )
    );
    assert_eq!(fields, BTreeSet::from(["op", "anchor", "callee"]));
}

#[test]
fn a_repoint_call_without_a_callee_is_refused_naming_the_field() {
    // Given a re-point that names no callee
    let plan = a_plan_of_one("repoint_call", A_CALL_IN_ROSTER_WORK, "");

    // When it is read
    let refusal = refusal_of(&plan);

    // Then the plan is malformed and the refusal names the missing field
    assert!(
        refusal.starts_with("plan is malformed:")
            && refusal.contains("`repoint_call` needs `callee`"),
        "unexpected refusal: {refusal}"
    );
}

#[test]
fn a_callee_that_is_not_one_path_or_method_chain_is_refused_naming_it() {
    // Given texts that are a call, a method call, an operator, a block, a closure, two expressions,
    // a macro, nothing, and a method turbofish with no call
    let not_chains = [
        "f(x)",
        "a.b()",
        "a + b",
        "{ a }.b",
        "|x| x",
        "a.b; c.d",
        "a::b!()",
        "",
        "self.m::<T>",
    ];

    // When each is read as the callee of a single-form re-point
    let refusals =
        not_chains.map(|text| refusal_of(&a_repoint_to(A_CALL_IN_ROSTER_WORK, text, "")));

    // Then every one is malformed, says what a callee is, and names the text it refused
    for (text, refusal) in not_chains.iter().zip(&refusals) {
        assert!(
            refusal.starts_with("plan is malformed:")
                && refusal.contains("`callee` must be one path or field/method chain")
                && refusal.contains(&format!("`{text}`")),
            "unexpected refusal for `{text}`: {refusal}"
        );
    }
}

#[test]
fn a_callee_may_hold_paths_fields_calls_and_a_qualified_self_type() {
    // Given callees that are a field chain, a chain through a call, a path with a turbofish, a
    // qualified self type, a dereferenced receiver and a tuple index
    let chains = [
        "self.peer.f",
        "self.agent_roster().f",
        "a::b::f::<T>",
        "<Host as Tr>::f",
        "(*self.x).f",
        "x.0.f",
    ];

    // When each is read as the callee of a single-form re-point
    let read = chains.map(|text| {
        the_only_operation_of(&a_repoint_to(A_CALL_IN_ROSTER_WORK, text, ""))
            .callee
            .expect("the callee is kept")
    });

    // Then every one is a plan, kept as written
    assert_eq!(read, chains);
}

#[test]
fn a_bulk_callee_must_start_with_the_receiver_and_end_with_the_methods_own_name() {
    // Given templates that name no receiver, only the receiver, put it mid-chain, name it twice, or
    // end in another method's name
    let refused = [
        "peer.m",
        "$receiver",
        "x.$receiver.m",
        "$receiver.a.$receiver.m",
        "$receiver.peer.other",
    ];
    let accepted = ["$receiver.peer.m", "$receiver.a().m"];

    // When each is read as the callee of a bulk re-point of `Host::m`
    let refusals = refused.map(|text| refusal_of(&a_repoint_to(A_METHOD, text, "")));
    let read = accepted.map(|text| {
        the_only_operation_of(&a_repoint_to(A_METHOD, text, ""))
            .callee
            .expect("the callee is kept")
    });

    // Then each refusal names the template it refused and the method, and the others are plans
    for (text, refusal) in refused.iter().zip(&refusals) {
        assert!(
            refusal.starts_with("plan is malformed:")
                && refusal.contains("`$receiver<hops>.m`")
                && refusal.contains(&format!("`{text}`")),
            "unexpected refusal for `{text}`: {refusal}"
        );
    }
    assert_eq!(read, accepted);
}

#[test]
fn a_single_callee_may_not_name_the_receiver_placeholder() {
    // Given a single-form re-point whose callee is a bulk template
    let plan = a_repoint_to(A_CALL_IN_ROSTER_WORK, "$receiver.peer.f", "");

    // When it is read
    let refusal = refusal_of(&plan);

    // Then the placeholder is named as the bulk form's
    assert!(
        refusal.starts_with("plan is malformed:")
            && refusal.contains("`$receiver` is the placeholder of the bulk form"),
        "unexpected refusal: {refusal}"
    );
}

#[test]
fn a_single_anchor_without_a_range_and_a_bulk_anchor_with_one_are_refused() {
    // Given a single-form callee over an anchor with no range, a template over an anchor with a
    // range, and a re-point anchored on a symbol and on a run of items
    let no_range = a_repoint_to(A_METHOD, "self.peer.f", "");
    let with_a_range = a_repoint_to(A_CALL_IN_ROSTER_WORK, "$receiver.peer.roster_work", "");
    let on_a_symbol = a_repoint_to(A_SYMBOL, "self.peer.f", "");
    let on_items = a_repoint_to(ITEMS, "self.peer.f", "");

    // When each is read
    let refusals = [
        refusal_of(&no_range),
        refusal_of(&with_a_range),
        refusal_of(&on_a_symbol),
        refusal_of(&on_items),
    ];

    // Then the first two say which form their callee belongs to, the others that the anchor is by item
    assert!(
        refusals[0].contains("`$receiver<hops>.m`") && refusals[1].contains("`$receiver`"),
        "unexpected refusals: {refusals:?}"
    );
    assert!(
        refusals[2..]
            .iter()
            .all(|refusal| refusal.contains("`repoint_call` anchors by item")),
        "unexpected refusals: {refusals:?}"
    );
}

#[test]
fn repoint_call_refuses_every_field_it_cannot_honour() {
    // Given a re-point carrying, in turn, each field only another operation honours
    let others = [
        ("variant", "\"first\""),
        ("name", "\"x\""),
        ("to", "\"app::roster\""),
        ("reexport", "\"glob\""),
        ("type", "\"u32\""),
        ("expr", "\"1\""),
        ("order", "[1]"),
        ("also", &format!("[{A_METHOD}]")),
        ("to_file", "true"),
        ("with_private_deps", "true"),
    ];

    // When each is read
    let refusals = others.iter().map(|(field, value)| {
        refusal_of(&a_repoint_to(
            A_CALL_IN_ROSTER_WORK,
            "self.peer.f",
            &format!(",\"{field}\":{value}"),
        ))
    });

    // Then every one is malformed, names its field and says the operation cannot honour it
    for ((field, _), refusal) in others.iter().zip(refusals) {
        assert!(
            refusal.starts_with("plan is malformed:")
                && refusal.contains(&format!("`{field}`"))
                && refusal.contains("cannot"),
            "unexpected refusal for `{field}`: {refusal}"
        );
    }
}

#[test]
fn callee_is_refused_on_every_other_operation() {
    // Given five other operations, each otherwise sound and each carrying a `callee`
    let carrying = |op: &str, anchor: &str, extra: &str| {
        a_plan_of_one(op, anchor, &format!("{extra},\"callee\":\"a.b\""))
    };
    let range = r#"{"kind":"range","file":"src/host.rs","start":{"line":1,"col":1},"end":{"line":3,"col":2}}"#;
    let plans = [
        carrying("rename_symbol", A_SYMBOL, ",\"name\":\"Roster\""),
        carrying("extract_method", range, ",\"name\":\"g\""),
        carrying("move_item", ITEMS, ",\"to\":\"app::roster\""),
        carrying(
            "add_call_arg",
            A_CALL_IN_ROSTER_WORK,
            ",\"variant\":\"last\",\"expr\":\"1\"",
        ),
        carrying(
            "retarget_impl",
            ITEMS,
            ",\"to_type\":\"app::roster::Roster\"",
        ),
    ];

    // When each is read
    let refusals = plans.map(|plan| refusal_of(&plan));

    // Then each is refused for the field, which only `repoint_call` honours
    for refusal in &refusals {
        assert!(
            refusal.starts_with("plan is malformed:")
                && refusal.contains("`callee` names the callee a call is re-pointed to")
                && refusal.contains("only `repoint_call` honours"),
            "unexpected refusal: {refusal}"
        );
    }
}

/// The op a range-anchored `repoint_call` of `callee` over `start`..`end` of `src/lib.rs` is.
fn a_range_anchored_repoint(start: (u32, u32), end: (u32, u32), callee: &str) -> RefactorOp {
    serde_json::from_value(serde_json::json!({
        "op": "repoint_call",
        "anchor": {
            "kind": "range",
            "file": "src/lib.rs",
            "start": { "line": start.0, "col": start.1 },
            "end": { "line": end.0, "col": end.1 },
        },
        "callee": callee,
    }))
    .expect("a `repoint_call` operation parses")
}

#[tokio::test]
async fn a_static_check_of_a_range_anchored_repoint_call_over_text_that_is_not_one_call_is_refused_with_no_server(
) {
    // Given a crate whose function adds two calls, and a re-point whose range covers the sum
    let workspace = an_app_holding(&[(
        "src/lib.rs",
        "pub fn run(a: u32, b: u32) -> u32 {\n    f(a) + g(b)\n}\nfn f(a: u32) -> u32 {\n    a\n}\nfn g(b: u32) -> u32 {\n    b\n}\n",
    )]);
    let op = a_range_anchored_repoint((2, 5), (2, 16), "h");

    // When a static check, which starts no server, examines it
    let findings = what_a_static_check_finds_in(&workspace, op).await;

    // Then it says the range is not one call
    assert!(
        findings
            .iter()
            .any(|finding| finding.contains("is not a call expression")),
        "no finding says the range is not one call: {findings:?}"
    );
}

#[tokio::test]
async fn a_static_check_refuses_a_callee_equal_to_the_current_one_and_an_old_callee_holding_a_call()
{
    // Given a crate calling `slot(x)` and `a.m(x).n(y)`
    let workspace = an_app_holding(&[(
        "src/lib.rs",
        "pub fn run(x: u32) {\n    slot(x);\n    a().m(x).n(x);\n}\n",
    )]);
    let to_itself = a_range_anchored_repoint((2, 5), (2, 12), "slot");
    let dropping_an_argument = a_range_anchored_repoint((3, 5), (3, 18), "b.n");

    // When a static check examines each
    let to_itself = what_a_static_check_finds_in(&workspace, to_itself).await;
    let dropping_an_argument = what_a_static_check_finds_in(&workspace, dropping_an_argument).await;

    // Then the first says nothing would change, and the second names the call whose arguments would be lost
    assert!(
        to_itself
            .iter()
            .any(|finding| finding.contains("re-points nothing")),
        "no finding says the callee is the current one: {to_itself:?}"
    );
    assert!(
        dropping_an_argument
            .iter()
            .any(|finding| finding.contains("holds a call") && finding.contains("m(x)")),
        "no finding names the call in the old callee: {dropping_an_argument:?}"
    );
}

#[tokio::test]
async fn a_static_check_of_an_item_anchored_repoint_call_says_to_run_deep() {
    // Given a re-point anchored by item, which only a deep check can resolve
    let workspace = an_app_holding(&[("src/lib.rs", "pub mod host;\n"), ("src/host.rs", "")]);
    let op: RefactorOp = serde_json::from_str(&format!(
        "{{\"op\":\"repoint_call\",\"anchor\":{A_CALL_IN_ROSTER_WORK},\"callee\":\"self.peer.f\"}}"
    ))
    .expect("a `repoint_call` operation parses");

    // When a plain check examines it
    let findings = what_a_static_check_finds_in(&workspace, op).await;

    // Then it is reported as not examined, naming the operation and the way to examine it
    assert!(
        findings
            .iter()
            .any(|finding| finding.contains("RepointCall")
                && finding.contains("anchors by item, which only a deep check can resolve")
                && finding.contains("run `check --deep`")),
        "the re-point passed a plain check without being examined: {findings:?}"
    );
}

//! The plan line of `detach_method`, and the refusals reading it makes before any server exists.
//!
//! `detach_method` turns members of one inherent `impl` into free functions over a named parameter.
//! Its line carries an anchor on the members, by item, and `name`, the parameter the receiver
//! becomes. Everything here is answered by the plan alone (DP1-DP3, `plan is malformed:`).

use std::collections::BTreeSet;

use tddy_code_restructuring::{Plan, RefactorKind};

const A_HEADER: &str = "{\"v\":1,\"snapshot\":{}}";

const ITEMS: &str = r#"{"kind":"items","file":"src/host.rs","items":["app::host::Host::bump","app::host::Host::twice"],"fingerprints":["sha256:a","sha256:b"]}"#;
const AN_ITEM: &str = r#"{"kind":"item","item":"app::host::Host::bump","file":"src/host.rs","fingerprint":"sha256:a"}"#;
const A_SYMBOL: &str = r#"{"kind":"symbol","file":"src/host.rs","path":"Host"}"#;
const A_RANGE: &str =
    r#"{"kind":"range","file":"src/host.rs","start":{"line":7,"col":5},"end":{"line":10,"col":6}}"#;

const THE_SESSION: &str = ",\"name\":\"session\"";

/// A plan of one `detach_method` line anchored on `anchor`, with the `extra` fields after it.
fn a_detach_line(anchor: &str, extra: &str) -> String {
    format!("{A_HEADER}\n{{\"op\":\"detach_method\",\"anchor\":{anchor}{extra}}}\n")
}

/// What reading the plan refuses with, or fails saying it did not refuse.
fn refusal_of(plan: &str) -> String {
    match Plan::parse(plan) {
        Err(refusal) => refusal.to_string(),
        Ok(_) => panic!("the plan was expected to be refused, and parsed:\n{plan}"),
    }
}

#[test]
fn a_detach_method_line_parses_and_reads_back_without_its_defaults() {
    // Given a detach line carrying only what it needs, on items and on one item
    let plans = [
        a_detach_line(ITEMS, THE_SESSION),
        a_detach_line(AN_ITEM, THE_SESSION),
    ];

    // When each is parsed and written back
    let read: Vec<(RefactorKind, Option<String>, BTreeSet<String>)> = plans
        .iter()
        .map(|plan| {
            let parsed = Plan::parse(plan).expect("the plan parses");
            let written = parsed.to_jsonl();
            let line: serde_json::Value =
                serde_json::from_str(written.lines().nth(1).expect("the operation's line"))
                    .expect("the line is JSON");
            let fields = line
                .as_object()
                .expect("an object")
                .keys()
                .cloned()
                .collect();
            (parsed.ops[0].op, parsed.ops[0].name.clone(), fields)
        })
        .collect();

    // Then each is the detach, naming its parameter, and no empty field is written
    let expected = (
        RefactorKind::DetachMethod,
        Some("session".to_string()),
        BTreeSet::from(["op".to_string(), "anchor".to_string(), "name".to_string()]),
    );
    assert_eq!(read, vec![expected.clone(), expected]);
}

#[test]
fn a_detach_method_without_a_name_or_with_self_a_keyword_or_two_words_as_its_name_is_malformed() {
    // Given a detach with no parameter, and four names that cannot be one
    let without_name = refusal_of(&a_detach_line(ITEMS, ""));
    let names = ["self", "match", "two words", "a.b"];
    let bad_names: Vec<String> = names
        .iter()
        .map(|name| {
            refusal_of(&a_detach_line(
                ITEMS,
                &format!(",\"name\":{}", serde_json::json!(name)),
            ))
        })
        .collect();

    // When each is read
    // Then the missing one names what it lacks, and each bad one is named
    assert!(
        without_name.starts_with("plan is malformed:")
            && without_name
                .contains("`detach_method` needs `name`: the parameter the receiver becomes"),
        "unexpected refusal: {without_name}"
    );
    let unnamed: Vec<(&&str, &String)> = names
        .iter()
        .zip(&bad_names)
        .filter(|(name, refusal)| {
            !(refusal.starts_with("plan is malformed:")
                && refusal.contains(&format!(
                    "`name` must be one identifier other than `self`; `{name}` is not"
                )))
        })
        .collect();
    assert!(unnamed.is_empty(), "unexpected refusals: {unnamed:?}");
}

#[test]
fn a_symbol_or_a_range_anchor_names_no_members_to_detach() {
    // Given a detach anchored on a symbol, and one anchored on a range
    let refusals = [
        ("symbol", refusal_of(&a_detach_line(A_SYMBOL, THE_SESSION))),
        ("range", refusal_of(&a_detach_line(A_RANGE, THE_SESSION))),
    ];

    // When each is read
    // Then each says the operation anchors on members, naming the kind it got
    let unexplained: Vec<&(&str, String)> = refusals
        .iter()
        .filter(|(kind, refusal)| {
            !(refusal.starts_with("plan is malformed:")
                && refusal.contains(&format!(
                    "`detach_method` anchors on members of one inherent `impl` (an `items` or `item` anchor): `{kind}` names no members"
                )))
        })
        .collect();
    assert!(
        unexplained.is_empty(),
        "unexpected refusals: {unexplained:?}"
    );
}

#[test]
fn every_field_detach_method_does_not_define_is_refused_naming_it() {
    // Given a detach carrying, one at a time, every field it has no use for
    let extras = [
        ("to", ",\"to\":\"app::other\""),
        ("to_type", ",\"to_type\":\"app::host::Roster\""),
        ("variant", ",\"variant\":\"leave_delegator\""),
        ("expr", ",\"expr\":\"self\""),
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
                refusal_of(&a_detach_line(ITEMS, &format!("{THE_SESSION}{extra}"))),
            )
        })
        .collect();

    // Then each is malformed and names its own field as one `detach_method` does not define
    let unnamed: Vec<&(&str, String)> = refusals
        .iter()
        .filter(|(field, refusal)| {
            !(refusal.starts_with("plan is malformed:")
                && refusal.contains(&format!("`{field}` is not a field of `detach_method`")))
        })
        .collect();
    assert!(
        unnamed.is_empty(),
        "these refusals do not name their field: {unnamed:?}"
    );
}

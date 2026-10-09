//! The plan line of `move_impl_members`, read without a language server.
//!
//! Everything here is answered by the text of the plan alone, before any index exists: the line
//! reads back as written, and every refusal it can earn from the plan (`plan is malformed:`, fixed
//! by editing the plan) names the operation and what is wrong with the line.

use std::collections::BTreeSet;

use tddy_code_restructuring::{Plan, RefactorKind};

const A_HEADER: &str = "{\"v\":1,\"snapshot\":{}}";

const MEMBERS: &str = r#"{"kind":"items","file":"src/host.rs","items":["app::host::Host::put","app::host::Host::last"],"fingerprints":["sha256:a","sha256:b"]}"#;
const A_WHOLE_BLOCK: &str =
    r#"{"kind":"item","item":"app::host::<Host>","file":"src/host.rs","fingerprint":"sha256:a"}"#;
const A_SYMBOL: &str = r#"{"kind":"symbol","file":"src/host.rs","path":"Host"}"#;
const A_RANGE: &str =
    r#"{"kind":"range","file":"src/host.rs","start":{"line":1,"col":1},"end":{"line":3,"col":2}}"#;
const A_TRAIT_IMPL_MEMBER: &str = r#"{"kind":"item","item":"app::host::<Host as Display>::fmt","file":"src/host.rs","fingerprint":"sha256:a"}"#;

/// A plan of one `move_impl_members` line anchored on `anchor`, with the `extra` fields after it.
fn a_member_move(anchor: &str, extra: &str) -> String {
    format!("{A_HEADER}\n{{\"op\":\"move_impl_members\",\"anchor\":{anchor}{extra}}}\n")
}

/// What reading the plan refuses with, or fails saying it did not refuse.
fn refusal_of(plan: &str) -> String {
    match Plan::parse(plan) {
        Err(refusal) => refusal.to_string(),
        Ok(_) => panic!("the plan was expected to be refused, and parsed:\n{plan}"),
    }
}

/// The fields the operation's line is written back with.
fn the_fields_written_back(plan: &str) -> (RefactorKind, BTreeSet<String>) {
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
    (parsed.ops[0].op, fields)
}

fn fields(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

#[test]
fn reads_a_member_run_line_with_its_destination() {
    // Given a line moving two members into an existing module, and one creating the module
    let into_an_existing_module = a_member_move(MEMBERS, ",\"to\":\"app::roster\"");
    let into_a_new_module = a_member_move(MEMBERS, ",\"name\":\"roster\",\"to\":\"app\"");

    // When each is parsed and written back
    let existing = the_fields_written_back(&into_an_existing_module);
    let created = the_fields_written_back(&into_a_new_module);

    // Then each is the member move, carrying only what it was written with
    assert_eq!(
        existing,
        (
            RefactorKind::MoveImplMembers,
            fields(&["op", "anchor", "to"])
        )
    );
    assert_eq!(
        created,
        (
            RefactorKind::MoveImplMembers,
            fields(&["op", "anchor", "name", "to"])
        )
    );
}

#[test]
fn refuses_a_line_without_a_destination() {
    // Given a member move that names no destination
    let plan = a_member_move(MEMBERS, "");

    // When it is read
    let refusal = refusal_of(&plan);

    // Then the plan is malformed and the refusal names the missing field
    assert!(
        refusal.starts_with("plan is malformed:")
            && refusal.contains("`move_impl_members` needs `to`"),
        "unexpected refusal: {refusal}"
    );
}

#[test]
fn refuses_a_range_or_symbol_anchor() {
    // Given a member move anchored by range and by symbol
    let anchors = [A_RANGE, A_SYMBOL];

    // When each is read
    let refusals =
        anchors.map(|anchor| refusal_of(&a_member_move(anchor, ",\"to\":\"app::roster\"")));

    // Then each is malformed: the operation anchors by item
    for refusal in &refusals {
        assert!(
            refusal.starts_with("plan is malformed:")
                && refusal.contains("`move_impl_members` anchors by item"),
            "unexpected refusal: {refusal}"
        );
    }
}

#[test]
fn refuses_a_whole_block_anchor_and_points_at_move_item() {
    // Given a member move anchored on the whole block `<Host>`
    let plan = a_member_move(A_WHOLE_BLOCK, ",\"to\":\"app::roster\"");

    // When it is read
    let refusal = refusal_of(&plan);

    // Then the refusal says a whole block moves with `move_item`
    assert!(
        refusal.starts_with("plan is malformed:")
            && refusal.contains("`move_impl_members`")
            && refusal.contains("`<Host>`")
            && refusal.contains("a whole `impl` block moves with `move_item`"),
        "unexpected refusal: {refusal}"
    );
}

#[test]
fn refuses_a_trait_member_path() {
    // Given a member move anchored on a member of `impl Display for Host`
    let plan = a_member_move(A_TRAIT_IMPL_MEMBER, ",\"to\":\"app::roster\"");

    // When it is read
    let refusal = refusal_of(&plan);

    // Then the refusal says a trait `impl` moves whole
    assert!(
        refusal.starts_with("plan is malformed:")
            && refusal.contains("`move_impl_members`")
            && refusal.contains("`<Host as Display>::fmt`")
            && refusal.contains("a trait `impl` moves whole with `move_item`"),
        "unexpected refusal: {refusal}"
    );
}

#[test]
fn refuses_every_field_the_operation_cannot_honour() {
    // Given a member move carrying, in turn, each field it has no use for
    let carried = [
        ("reexport", ",\"reexport\":\"glob\""),
        ("canonical_paths", ",\"canonical_paths\":true"),
        ("to_type", ",\"to_type\":\"app::roster::Roster\""),
        ("to_file", ",\"to_file\":true"),
        (
            "also",
            ",\"also\":[{\"kind\":\"symbol\",\"file\":\"src/a.rs\",\"path\":\"a\"}]",
        ),
        ("callee", ",\"callee\":\"self.f\""),
    ];

    // When each is read
    let refusals = carried.map(|(field, extra)| {
        (
            field,
            refusal_of(&a_member_move(
                MEMBERS,
                &format!(",\"to\":\"app::roster\"{extra}"),
            )),
        )
    });

    // Then every one is malformed and names the operation and the field
    for (field, refusal) in &refusals {
        assert!(
            refusal.starts_with("plan is malformed:")
                && refusal.contains(&format!("`move_impl_members` cannot honour `{field}`")),
            "`{field}` was refused as: {refusal}"
        );
    }
}

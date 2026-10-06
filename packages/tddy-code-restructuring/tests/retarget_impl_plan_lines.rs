//! The plan line of `retarget_impl`, read without a language server.
//!
//! Everything here is answered by the text of the plan, the manifests and one module's source, so
//! it is answered in milliseconds and before any index exists: the parse-time refusals (`plan is
//! malformed:`, fixed by editing the plan) and the two static findings a plain `check` makes.

mod harness;
mod same_crate;

use std::collections::BTreeSet;

use harness::AFixtureWorkspace;
use same_crate::{an_app_holding, an_app_over_a_kernel, what_a_static_check_finds_in};
use tddy_code_restructuring::{Plan, RefactorKind, RefactorOp};

const A_HEADER: &str = "{\"v\":1,\"snapshot\":{}}";

const ITEMS: &str = r#"{"kind":"items","file":"src/host.rs","items":["app::host::Host::put","app::host::Host::last"],"fingerprints":["sha256:a","sha256:b"]}"#;
const A_WHOLE_BLOCK: &str =
    r#"{"kind":"item","item":"app::host::<Host>","file":"src/host.rs","fingerprint":"sha256:a"}"#;
const A_SYMBOL: &str = r#"{"kind":"symbol","file":"src/host.rs","path":"Host"}"#;
const A_RANGE: &str =
    r#"{"kind":"range","file":"src/host.rs","start":{"line":1,"col":1},"end":{"line":3,"col":2}}"#;
const A_TRAIT_IMPL_MEMBER: &str = r#"{"kind":"item","item":"app::host::<Host as Display>::fmt","file":"src/host.rs","fingerprint":"sha256:a"}"#;
const A_MEMBER_WITH_A_RELATIVE_RANGE: &str = r#"{"kind":"item","item":"app::host::Host::put","file":"src/host.rs","start":{"line":1,"col":1},"end":{"line":2,"col":2},"fingerprint":"sha256:a"}"#;

/// A plan of one line: `op`, anchored on `anchor`, with the `extra` fields written after them.
fn a_plan_of_one(op: &str, anchor: &str, extra: &str) -> String {
    format!("{A_HEADER}\n{{\"op\":\"{op}\",\"anchor\":{anchor}{extra}}}\n")
}

fn a_retarget_to(anchor: &str, to_type: &str) -> String {
    a_plan_of_one(
        "retarget_impl",
        anchor,
        &format!(",\"to_type\":{}", serde_json::json!(to_type)),
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

/// The fixture of the static findings: `host.rs` holds an `impl Host`, `roster.rs` declares
/// `roster_text`.
fn an_app_whose_roster_module_reads(roster_text: &str) -> AFixtureWorkspace {
    an_app_holding(&[
        ("src/lib.rs", "pub mod host;\npub mod roster;\n"),
        (
            "src/host.rs",
            "pub struct Host {\n    pub(crate) n: u32,\n}\n\nimpl Host {\n    pub fn put(&mut self, v: u32) {\n        self.n = v;\n    }\n}\n",
        ),
        ("src/roster.rs", roster_text),
    ])
}

fn a_retarget_op(anchor: &str, to_type: &str) -> RefactorOp {
    serde_json::from_str(&format!(
        "{{\"op\":\"retarget_impl\",\"anchor\":{anchor},\"to_type\":{}}}",
        serde_json::json!(to_type)
    ))
    .expect("a `retarget_impl` operation parses")
}

const AN_ITEM_ANCHOR_OVER_PUT: &str = r#"{"kind":"item","item":"app::host::Host::put","file":"src/host.rs","fingerprint":"sha256:a"}"#;

#[test]
fn a_retarget_impl_line_parses_and_reads_back_without_its_defaults() {
    // Given a retarget line carrying only what it needs
    let plan = a_retarget_to(ITEMS, "app::roster::Roster");

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

    // Then it is the retarget operation, naming the new type, and no empty field is written
    assert_eq!(
        (parsed.ops[0].op, parsed.ops[0].to_type.as_deref()),
        (RefactorKind::RetargetImpl, Some("app::roster::Roster"))
    );
    assert_eq!(fields, BTreeSet::from(["op", "anchor", "to_type"]));
}

#[test]
fn a_retarget_without_to_type_is_malformed() {
    // Given a retarget that names no new type
    let plan = a_plan_of_one("retarget_impl", ITEMS, "");

    // When it is read
    let refusal = refusal_of(&plan);

    // Then the plan is malformed and the refusal names the missing field
    assert!(
        refusal.starts_with("plan is malformed:")
            && refusal.contains("`retarget_impl` needs `to_type`: the type the members move to"),
        "unexpected refusal: {refusal}"
    );
}

#[test]
fn to_type_must_be_one_path_type() {
    // Given four `to_type`s that are not one path type: a reference, a trait object, a list and a tuple
    let not_paths = [
        "&Roster",
        "dyn Roster",
        "app::A, app::B",
        "(app::A, app::B)",
    ];

    // When each is read
    let refusals = not_paths.map(|text| refusal_of(&a_retarget_to(ITEMS, text)));

    // Then every one is malformed and says what `to_type` is
    for (text, refusal) in not_paths.iter().zip(&refusals) {
        assert!(
            refusal.starts_with("plan is malformed:")
                && refusal.contains("`to_type` must be one path type")
                && refusal.contains(text),
            "`{text}` was refused as: {refusal}"
        );
    }
}

#[test]
fn to_type_on_another_operation_is_refused_naming_it() {
    // Given a `move_item` that carries a `to_type`
    let plan = a_plan_of_one(
        "move_item",
        ITEMS,
        ",\"to\":\"app::roster\",\"to_type\":\"app::roster::Roster\"",
    );

    // When it is read
    let refusal = refusal_of(&plan);

    // Then the refusal says which operation honours the field, and which one was asked
    assert!(
        refusal.starts_with("plan is malformed:")
            && refusal.contains("only `retarget_impl` honours")
            && refusal.contains("MoveItem"),
        "unexpected refusal: {refusal}"
    );
}

#[test]
fn a_range_or_symbol_anchor_names_no_impl_to_retarget() {
    // Given a retarget anchored by range, by symbol, and by an item with a relative range
    let anchors = [A_RANGE, A_SYMBOL, A_MEMBER_WITH_A_RELATIVE_RANGE];

    // When each is read
    let refusals = anchors.map(|anchor| refusal_of(&a_retarget_to(anchor, "app::roster::Roster")));

    // Then each is malformed, and says an item anchor is what a retarget needs
    for refusal in &refusals {
        assert!(
            refusal.starts_with("plan is malformed:")
                && refusal.contains("`retarget_impl` anchors by item"),
            "unexpected refusal: {refusal}"
        );
    }
}

#[test]
fn a_trait_impl_path_is_refused_at_parse_time() {
    // Given a retarget anchored on a member of a trait impl
    let plan = a_retarget_to(A_TRAIT_IMPL_MEMBER, "app::roster::Roster");

    // When it is read
    let refusal = refusal_of(&plan);

    // Then the plan is refused for naming a trait impl, with the path it named
    assert!(
        refusal.starts_with("plan is malformed:")
            && refusal.contains("is a trait impl")
            && refusal.contains("app::host::<Host as Display>::fmt"),
        "unexpected refusal: {refusal}"
    );
}

#[test]
fn to_and_name_are_not_fields_of_retarget_impl_and_the_refusal_points_at_to_type() {
    // Given a retarget that also carries `to`, and another that carries `name`
    let with_to = a_plan_of_one(
        "retarget_impl",
        ITEMS,
        ",\"to_type\":\"app::roster::Roster\",\"to\":\"app::roster\"",
    );
    let with_name = a_plan_of_one(
        "retarget_impl",
        ITEMS,
        ",\"to_type\":\"app::roster::Roster\",\"name\":\"Roster\"",
    );

    // When each is read
    let refusals = [refusal_of(&with_to), refusal_of(&with_name)];

    // Then each names its own field and points at `to_type`
    assert!(
        refusals[0].contains("`to` is not a field of `retarget_impl`")
            && refusals[1].contains("`name` is not a field of `retarget_impl`")
            && refusals.iter().all(|refusal| refusal.contains("`to_type`")),
        "unexpected refusals: {refusals:?}"
    );
}

#[tokio::test]
async fn a_to_type_in_another_package_is_refused_by_a_static_check() {
    // Given an `app` package over a `kernel` package, and a retarget of `app`'s impl to `kernel`'s type
    let workspace = an_app_over_a_kernel(
        &[
            ("src/lib.rs", "pub mod host;\n"),
            (
                "src/host.rs",
                "pub struct Host;\n\nimpl Host {\n    pub fn put(&self) {}\n}\n",
            ),
        ],
        &[
            ("src/lib.rs", "pub mod roster;\n"),
            ("src/roster.rs", "pub struct Roster;\n"),
        ],
    );
    let anchor = AN_ITEM_ANCHOR_OVER_PUT.replace("src/host.rs", "app/src/host.rs");

    // When a static check, which starts no server, examines it
    let findings =
        what_a_static_check_finds_in(&workspace, a_retarget_op(&anchor, "kernel::roster::Roster"))
            .await;

    // Then it says the type is in another package and the operation does not leave its crate
    assert!(
        findings.iter().any(|finding| finding.contains(
            "`to_type` is in package `kernel`, and the anchor is in `app`: `retarget_impl` does not leave its crate"
        )),
        "no finding names the other package: {findings:?}"
    );
}

#[tokio::test]
async fn a_to_type_whose_module_declares_no_such_type_is_refused_by_a_static_check() {
    // Given a `roster` module that declares no `Roster`
    let workspace = an_app_whose_roster_module_reads("pub struct Other;\n");

    // When a static check examines a retarget to `app::roster::Roster`
    let findings = what_a_static_check_finds_in(
        &workspace,
        a_retarget_op(AN_ITEM_ANCHOR_OVER_PUT, "app::roster::Roster"),
    )
    .await;

    // Then it says the module declares no such type
    assert!(
        findings.iter().any(|finding| finding
            .contains("`app::roster` declares no struct, enum or union named `Roster`")),
        "no finding names the missing type: {findings:?}"
    );
}

#[tokio::test]
async fn a_plain_check_reports_an_item_anchored_retarget_as_unexamined_rather_than_passing_it() {
    // Given a retarget anchored by item, which only a deep check can resolve
    let workspace =
        an_app_whose_roster_module_reads("pub struct Roster {\n    pub(crate) n: u32,\n}\n");

    // When a plain check examines it
    let findings = what_a_static_check_finds_in(
        &workspace,
        a_retarget_op(AN_ITEM_ANCHOR_OVER_PUT, "app::roster::Roster"),
    )
    .await;

    // Then it is reported as not examined, with the way to examine it
    assert!(
        findings
            .iter()
            .any(|finding| finding.contains("RetargetImpl")
                && finding.contains("anchors by item, which only a deep check can resolve")
                && finding.contains("run `check --deep`")),
        "the retarget passed a plain check without being examined: {findings:?}"
    );
}

#[test]
fn a_whole_block_anchor_is_one_operation() {
    // Given a retarget anchored on a whole block
    let plan = a_retarget_to(A_WHOLE_BLOCK, "app::roster::Pair<T>");

    // When it is read
    let op = the_only_operation_of(&plan);

    // Then the generic arguments of the new type are kept as written
    assert_eq!(op.to_type.as_deref(), Some("app::roster::Pair<T>"));
}

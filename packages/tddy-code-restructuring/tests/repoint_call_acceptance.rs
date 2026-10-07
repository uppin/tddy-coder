//! `repoint_call` re-points a call's callee, or the receiver of every call of a method, against a
//! live rust-analyzer.
//!
//! After a method moves to another type its callers change by a few tokens that no call-site
//! operation expresses: `self.slot(x)` becomes `self.peer.slot(x)`, `x.m(..)` becomes
//! `x.agent_roster().m(..)`. The single form re-points one call, anchored on it; the bulk form is
//! anchored on the method and re-points every call the server knows, in any file of any package.
//!
//! `cargo check` is the assertion no edit that merely looks right can satisfy, and the files are
//! compared byte for byte so an edit that reaches past the callee shows.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use harness::{
    a_sink_that_keeps_what_it_hears, applying_a_plan_of, applying_the_plan_with,
    assert_compiles_with_its_tests, checking_the_plan, AFixtureWorkspace,
};
use same_crate::{an_app_holding, an_app_with_a_consumer, the_anchor_over};
use tddy_code_restructuring::{Anchor, Position, RefactorOp};

const PEER: &str = "pub struct Peer;\n\nimpl Peer {\n    pub fn slot(&self, x: u32) -> u32 {\n        x + 1\n    }\n}\n";

/// A `Host` holding a `Peer`, whose `slot` and `roster_work` call `slot` through `self`.
const A_HOST_CALLING_ITS_OWN_SLOT: &str = concat!(
    "use crate::peer::Peer;\n\n",
    "pub struct Host {\n    pub peer: Peer,\n}\n\n",
    "impl Host {\n",
    "    pub fn slot(&self, x: u32) -> u32 {\n        x\n    }\n\n",
    "    pub fn roster_work(&self, x: u32) -> u32 {\n",
    "        // kept: self.slot(1) is only a comment\n",
    "        let first = self.slot(x);\n",
    "        let second = self.slot(x + 1);\n",
    "        first + second\n",
    "    }\n",
    "}\n",
);

fn a_crate_of_host_and_peer(host: &str) -> AFixtureWorkspace {
    an_app_holding(&[
        ("src/lib.rs", "pub mod host;\npub mod peer;\n"),
        ("src/host.rs", host),
        ("src/peer.rs", PEER),
    ])
}

/// The `item` anchor over the one item `path` names in `file`, with no range.
///
/// The anchors command answers a name with an `items` anchor over a run of one; `repoint_call` is
/// anchored on the item itself (its name, for the bulk form; a relative range inside it, for the
/// single one), so the run of one is read as the item.
async fn the_item(workspace: &AFixtureWorkspace, file: &str, path: &str) -> Anchor {
    let Anchor::Items {
        file,
        mut items,
        mut fingerprints,
    } = the_anchor_over(workspace, file, &[path]).await
    else {
        panic!("the anchors command emits an `items` anchor over a name");
    };
    Anchor::Item {
        item: items.remove(0),
        file,
        start: None,
        end: None,
        fingerprint: fingerprints.remove(0),
        hint: None,
    }
}

/// The anchor over `method` of the `Host` in `file`, with the relative range of the first
/// occurrence of `call` in the file.
///
/// Lines are relative to the method's first line (the line of its `fn`), columns are the file's.
async fn the_call_in(
    workspace: &AFixtureWorkspace,
    file: &str,
    method: &str,
    call: &str,
) -> Anchor {
    let anchor = the_item(workspace, file, &format!("app::host::Host::{method}")).await;
    let text = workspace.read(file);
    let method_line = text
        .lines()
        .position(|line| line.contains(&format!("fn {method}(")))
        .expect("the method is declared in the file");
    let offset = text.find(call).expect("the call is in the file");
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let col = before.rsplit('\n').next().map_or(0, str::len);
    let at = |line: usize, col: usize| Position {
        line: (line - method_line) as u32 + 1,
        col: col as u32 + 1,
    };
    let Anchor::Item {
        item,
        file,
        fingerprint,
        ..
    } = anchor
    else {
        panic!("the anchors command emits an item anchor over one method");
    };
    Anchor::Item {
        item,
        file,
        start: Some(at(line, col)),
        end: Some(at(line, col + call.len())),
        fingerprint,
        hint: None,
    }
}

/// The anchor over the method `Host::<method>` in `file`, with no range: the bulk form's.
async fn the_method(workspace: &AFixtureWorkspace, file: &str, method: &str) -> Anchor {
    the_item(workspace, file, &format!("app::host::Host::{method}")).await
}

fn a_repoint_op(anchor: &Anchor, callee: &str, group: Option<&str>) -> RefactorOp {
    let mut op = serde_json::json!({ "op": "repoint_call", "anchor": anchor, "callee": callee });
    if let Some(group) = group {
        op["group"] = serde_json::json!(group);
    }
    serde_json::from_value(op).expect("a `repoint_call` operation parses")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_call_is_re_pointed_through_a_field_and_the_crate_still_compiles() {
    // Given a method calling `slot` twice through `self`, and a peer with the same method
    let workspace = a_crate_of_host_and_peer(A_HOST_CALLING_ITS_OWN_SLOT);
    let before = workspace.read("src/host.rs");
    let anchor = the_call_in(&workspace, "src/host.rs", "roster_work", "self.slot(x)").await;

    // When the first call is re-pointed through the `peer` field
    let applied =
        applying_a_plan_of(&workspace, &[a_repoint_op(&anchor, "self.peer.slot", None)]).await;

    // Then only that call changed, byte for byte, and the crate compiles with its tests
    applied.expect("the re-point applies");
    assert_eq!(
        workspace.read("src/host.rs"),
        before.replacen("self.slot(x)", "self.peer.slot(x)", 1)
    );
    assert_compiles_with_its_tests(&workspace);
}

const A_HOST_CALLING_A_FREE_SLOT: &str = concat!(
    "pub fn slot(a: u32, b: u32) -> u32 {\n    a + b\n}\n\n",
    "pub fn f(b: u32) -> u32 {\n    b * 2\n}\n\n",
    "pub fn roster_work(a: u32, b: u32) -> u32 {\n",
    "    slot(a, f(b))\n",
    "}\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn a_path_call_is_re_pointed_to_another_module_and_its_arguments_are_kept() {
    // Given a free function calling `slot(a, f(b))` and a `lookup` module with a `slot` of its own
    let workspace = an_app_holding(&[
        ("src/lib.rs", "pub mod host;\npub mod lookup;\n"),
        ("src/host.rs", A_HOST_CALLING_A_FREE_SLOT),
        (
            "src/lookup.rs",
            "pub fn slot(a: u32, b: u32) -> u32 {\n    a * b\n}\n",
        ),
    ]);
    let before = workspace.read("src/host.rs");
    let anchor = {
        let anchor = the_item(&workspace, "src/host.rs", "app::host::roster_work").await;
        let Anchor::Item {
            item,
            file,
            fingerprint,
            ..
        } = anchor
        else {
            panic!("the anchors command emits an item anchor over one function");
        };
        let line = A_HOST_CALLING_A_FREE_SLOT
            .lines()
            .position(|line| line.contains("slot(a, f(b))"))
            .expect("the call is in the function");
        let first = A_HOST_CALLING_A_FREE_SLOT
            .lines()
            .position(|line| line.contains("fn roster_work"))
            .expect("the function is declared");
        let relative = (line - first) as u32 + 1;
        Anchor::Item {
            item,
            file,
            start: Some(Position {
                line: relative,
                col: 5,
            }),
            end: Some(Position {
                line: relative,
                col: 18,
            }),
            fingerprint,
            hint: None,
        }
    };

    // When the call is re-pointed to the `lookup` module's
    let applied = applying_a_plan_of(
        &workspace,
        &[a_repoint_op(&anchor, "crate::lookup::slot", None)],
    )
    .await;

    // Then the callee changed and both arguments, the nested call included, are as they were
    applied.expect("the re-point applies");
    assert_eq!(
        workspace.read("src/host.rs"),
        before.replacen("slot(a, f(b))", "crate::lookup::slot(a, f(b))", 1)
    );
    assert_compiles_with_its_tests(&workspace);
}

const A_PEER_WITH_TWO_ARGUMENTS: &str = "pub struct Peer;\n\nimpl Peer {\n    pub fn slot(&self, x: u32, y: u32) -> u32 {\n        x + y\n    }\n}\n";

#[tokio::test(flavor = "multi_thread")]
async fn an_argument_added_and_a_callee_re_pointed_on_one_call_compile_as_one_group() {
    // Given a call of `self.slot(x)` that must become `self.peer.slot(x, 1)`: the peer's method takes
    // the extra argument and the host's does not
    let workspace = an_app_holding(&[
        ("src/lib.rs", "pub mod host;\npub mod peer;\n"),
        ("src/host.rs", A_HOST_CALLING_ITS_OWN_SLOT),
        ("src/peer.rs", A_PEER_WITH_TWO_ARGUMENTS),
    ]);
    let before = workspace.read("src/host.rs");
    let the_call = the_call_in(&workspace, "src/host.rs", "roster_work", "self.slot(x)").await;
    let adding = serde_json::from_value::<RefactorOp>(serde_json::json!({
        "op": "add_call_arg", "anchor": the_call, "variant": "last", "expr": "1", "group": "both",
    }))
    .expect("an `add_call_arg` operation parses");

    // When the argument is added, then the callee re-pointed, both over the call as first written
    // (arguments first: the order the ledger is expected to translate)
    let applied = applying_a_plan_of(
        &workspace,
        &[
            adding,
            a_repoint_op(&the_call, "self.peer.slot", Some("both")),
        ],
    )
    .await;

    // Then the call carries both changes and the group compiles
    applied.expect("the group applies");
    assert_eq!(
        workspace.read("src/host.rs"),
        before.replacen("self.slot(x)", "self.peer.slot(x, 1)", 1)
    );
    assert_compiles_with_its_tests(&workspace);
}

/// A `Host` whose `slot` is called through a variable, a call, `self` and a macro argument, from
/// three files; `Peer` has the same method.
fn a_crate_calling_slot_from_three_files() -> AFixtureWorkspace {
    an_app_holding(&[
        (
            "src/lib.rs",
            "pub mod callers;\npub mod host;\npub mod more;\npub mod peer;\n",
        ),
        (
            "src/host.rs",
            concat!(
                "use crate::peer::Peer;\n\n",
                "pub struct Host {\n    pub peer: Peer,\n}\n\n",
                "impl Host {\n",
                "    pub fn slot(&self, x: u32) -> u32 {\n        x\n    }\n\n",
                "    pub fn own(&self) -> u32 {\n        self.slot(2)\n    }\n\n",
                "    pub fn a(&self) -> &Host {\n        self\n    }\n",
                "}\n",
            ),
        ),
        (
            "src/callers.rs",
            concat!(
                "use crate::host::Host;\n\n",
                "pub fn direct(host: &Host) -> u32 {\n    host.slot(1)\n}\n\n",
                "pub fn through_a_call(h: &Host) -> u32 {\n    h.a().slot(3)\n}\n",
            ),
        ),
        (
            "src/more.rs",
            concat!(
                "use crate::host::Host;\n\n",
                "pub fn in_a_macro(host: &Host) -> String {\n    format!(\"{}\", host.slot(4))\n}\n",
            ),
        ),
        ("src/peer.rs", PEER),
    ])
}

#[tokio::test(flavor = "multi_thread")]
async fn every_method_call_of_a_method_has_its_receiver_re_pointed_across_files() {
    // Given `Host::slot` called from three files: through a variable, through a call, through
    // `self`, and inside `format!`
    let workspace = a_crate_calling_slot_from_three_files();
    let (host, callers, more) = (
        workspace.read("src/host.rs"),
        workspace.read("src/callers.rs"),
        workspace.read("src/more.rs"),
    );
    let anchor = the_method(&workspace, "src/host.rs", "slot").await;

    // When the receivers are given a `peer` hop
    let applied = applying_a_plan_of(
        &workspace,
        &[a_repoint_op(&anchor, "$receiver.peer.slot", None)],
    )
    .await;

    // Then every receiver gained `.peer` and nothing else changed, and the crate compiles
    applied.expect("the re-point applies");
    assert_eq!(
        workspace.read("src/host.rs"),
        host.replacen("self.slot(2)", "self.peer.slot(2)", 1)
    );
    assert_eq!(
        workspace.read("src/callers.rs"),
        callers
            .replacen("host.slot(1)", "host.peer.slot(1)", 1)
            .replacen("h.a().slot(3)", "h.a().peer.slot(3)", 1)
    );
    assert_eq!(
        workspace.read("src/more.rs"),
        more.replacen("host.slot(4)", "host.peer.slot(4)", 1)
    );
    assert_compiles_with_its_tests(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_bulk_form_edits_a_caller_in_another_crate_and_a_test_binary() {
    // Given `Host::slot` called from a consumer crate and from a test binary of its own crate
    let workspace = an_app_with_a_consumer(
        &[
            ("src/lib.rs", "pub mod host;\npub mod peer;\n"),
            (
                "src/host.rs",
                concat!(
                    "use crate::peer::Peer;\n\n",
                    "pub struct Host {\n    pub peer: Peer,\n}\n\n",
                    "impl Host {\n    pub fn slot(&self, x: u32) -> u32 {\n        x\n    }\n}\n",
                ),
            ),
            ("src/peer.rs", PEER),
            (
                "tests/uses_slot.rs",
                "use app::host::Host;\n\n#[test]\nfn slots() {\n    let host = Host {\n        peer: app::peer::Peer,\n    };\n    assert_eq!(host.slot(5), 5);\n}\n",
            ),
        ],
        &[(
            "src/lib.rs",
            "pub fn consume(host: &app::host::Host) -> u32 {\n    host.slot(1)\n}\n",
        )],
    );
    let (consumer, test) = (
        workspace.read("consumer/src/lib.rs"),
        workspace.read("app/tests/uses_slot.rs"),
    );
    let anchor = the_method(&workspace, "app/src/host.rs", "slot").await;

    // When the receivers are given a `peer` hop
    let applied = applying_a_plan_of(
        &workspace,
        &[a_repoint_op(&anchor, "$receiver.peer.slot", None)],
    )
    .await;

    // Then both gained it and the workspace compiles, tests included
    applied.expect("the re-point applies");
    assert_eq!(
        workspace.read("consumer/src/lib.rs"),
        consumer.replacen("host.slot(1)", "host.peer.slot(1)", 1)
    );
    assert_eq!(
        workspace.read("app/tests/uses_slot.rs"),
        test.replacen("host.slot(5)", "host.peer.slot(5)", 1)
    );
    assert_compiles_with_its_tests(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_nested_and_a_chained_call_of_one_method_are_both_re_pointed_without_overlap() {
    // Given `m` called inside the arguments of another call of `m`, and called on its own result
    let workspace = an_app_holding(&[
        ("src/lib.rs", "pub mod host;\npub mod peer;\npub mod use_m;\n"),
        (
            "src/host.rs",
            concat!(
                "use crate::peer::Peer;\n\n",
                "pub struct Host {\n    pub peer: Peer,\n}\n\n",
                "impl Host {\n",
                "    pub fn m<'a>(&self, other: &'a Host) -> &'a Host {\n        other\n    }\n",
                "}\n",
            ),
        ),
        (
            "src/peer.rs",
            "use crate::host::Host;\n\npub struct Peer;\n\nimpl Peer {\n    pub fn m<'a>(&self, other: &'a Host) -> &'a Host {\n        other\n    }\n}\n",
        ),
        (
            "src/use_m.rs",
            concat!(
                "use crate::host::Host;\n\n",
                "pub fn nested(a: &Host, b: &Host, c: &Host) -> usize {\n    a.m(b.m(c)) as *const Host as usize\n}\n\n",
                "pub fn chained(a: &Host, c: &Host) -> usize {\n    a.m(c).m(c) as *const Host as usize\n}\n",
            ),
        ),
    ]);
    let before = workspace.read("src/use_m.rs");
    let anchor = the_method(&workspace, "src/host.rs", "m").await;

    // When the receivers are given a `peer` hop
    let applied = applying_a_plan_of(
        &workspace,
        &[a_repoint_op(&anchor, "$receiver.peer.m", None)],
    )
    .await;

    // Then each call gained exactly one hop, the inner ones too, and the crate compiles
    applied.expect("the re-point applies");
    assert_eq!(
        workspace.read("src/use_m.rs"),
        before
            .replacen("a.m(b.m(c))", "a.peer.m(b.peer.m(c))", 1)
            .replacen("a.m(c).m(c)", "a.peer.m(c).peer.m(c)", 1)
    );
    assert_compiles_with_its_tests(&workspace);
}

/// A `Host::size` called, named as a path, and passed as a function.
fn a_crate_naming_size_three_ways() -> AFixtureWorkspace {
    an_app_holding(&[
        ("src/lib.rs", "pub mod host;\npub mod other;\npub mod peer;\n"),
        (
            "src/host.rs",
            concat!(
                "use crate::peer::Peer;\n\n",
                "pub struct Host {\n    pub peer: Peer,\n}\n\n",
                "impl Host {\n    pub fn size(&self) -> u32 {\n        1\n    }\n}\n",
            ),
        ),
        (
            "src/other.rs",
            concat!(
                "use crate::host::Host;\n\n",
                "pub fn called(h: &Host) -> u32 {\n    h.size()\n}\n\n",
                "pub fn by_path(h: &Host) -> u32 {\n    Host::size(h)\n}\n\n",
                "pub fn as_a_function(hosts: &[Host]) -> Vec<u32> {\n    hosts.iter().map(Host::size).collect()\n}\n",
            ),
        ),
        (
            "src/peer.rs",
            "pub struct Peer;\n\nimpl Peer {\n    pub fn size(&self) -> u32 {\n        2\n    }\n}\n",
        ),
    ])
}

fn every_file_of(workspace: &AFixtureWorkspace, files: &[&str]) -> Vec<String> {
    files.iter().map(|file| workspace.read(file)).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_site_that_is_not_a_method_call_is_refused_naming_every_one_and_nothing_is_written() {
    // Given `Host::size` called once as a method, named once as a path call and once passed as a function
    let workspace = a_crate_naming_size_three_ways();
    let files = ["src/host.rs", "src/other.rs", "src/peer.rs"];
    let before = every_file_of(&workspace, &files);
    let anchor = the_method(&workspace, "src/host.rs", "size").await;

    // When the receivers are re-pointed
    let refused = applying_a_plan_of(
        &workspace,
        &[a_repoint_op(&anchor, "$receiver.peer.size", None)],
    )
    .await;

    // Then the run is refused naming both sites that are not method calls, by file and line, and
    // the tree is byte-identical
    let refusal = refused.expect_err("a path call and a function pointer cannot be re-pointed");
    assert!(
        refusal.contains("this seam cannot be cut here:")
            && refusal.contains("src/other.rs:8")
            && refusal.contains("src/other.rs:12"),
        "the refusal does not name both sites: {refusal}"
    );
    assert_eq!(every_file_of(&workspace, &files), before);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reference_in_a_comment_is_left_alone_and_a_method_nothing_calls_is_a_no_op_with_a_note()
{
    // Given `Host::idle`, which nothing calls and one doc comment links to
    let workspace = an_app_holding(&[
        ("src/lib.rs", "pub mod host;\npub mod peer;\n"),
        (
            "src/host.rs",
            concat!(
                "use crate::peer::Peer;\n\n",
                "pub struct Host {\n    pub peer: Peer,\n}\n\n",
                "impl Host {\n    pub fn idle(&self) -> u32 {\n        0\n    }\n\n",
                "    /// Counts what [`Host::idle`] does not.\n",
                "    pub fn busy(&self) -> u32 {\n        1\n    }\n}\n",
            ),
        ),
        (
            "src/peer.rs",
            "pub struct Peer;\n\nimpl Peer {\n    pub fn idle(&self) -> u32 {\n        0\n    }\n}\n",
        ),
    ]);
    let files = ["src/host.rs", "src/peer.rs"];
    let before = every_file_of(&workspace, &files);
    let anchor = the_method(&workspace, "src/host.rs", "idle").await;
    let plan = workspace.a_plan_of(&[a_repoint_op(&anchor, "$receiver.peer.idle", None)]);
    let (heard, lines) = a_sink_that_keeps_what_it_hears();

    // When the receivers are re-pointed
    let applied = applying_the_plan_with(&workspace, plan, |options| options.account = heard).await;

    // Then the run succeeds and writes nothing, and says it found no call and skipped a comment
    applied.expect("a method nothing calls is not an error");
    assert_eq!(every_file_of(&workspace, &files), before);
    let said = lines.lock().expect("the lines are readable").join("\n");
    assert!(
        said.contains("idle") && said.contains("no call") && said.contains("comment"),
        "no note says why nothing changed: {said}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deep_check_refuses_what_an_apply_refuses_and_finds_nothing_in_a_good_plan() {
    // Given a crate whose `size` is also named as a path, and one whose `slot` is only called
    let refusing = a_crate_naming_size_three_ways();
    let sound = a_crate_calling_slot_from_three_files();
    let size = the_method(&refusing, "src/host.rs", "size").await;
    let slot = the_method(&sound, "src/host.rs", "slot").await;

    // When a deep check examines a re-point of each
    let refused = checking_the_plan(
        &refusing,
        refusing.a_plan_of(&[a_repoint_op(&size, "$receiver.peer.size", None)]),
        true,
    )
    .await
    .expect("a deep check runs");
    let found_nothing = checking_the_plan(
        &sound,
        sound.a_plan_of(&[a_repoint_op(&slot, "$receiver.peer.slot", None)]),
        true,
    )
    .await
    .expect("a deep check runs");

    // Then the first is refused as an apply would be, naming the sites, and the second finds nothing
    assert!(
        refused.iter().any(
            |finding| finding.contains("src/other.rs:8") && finding.contains("src/other.rs:12")
        ),
        "the deep check did not refuse the sites an apply refuses: {refused:?}"
    );
    assert_eq!(found_nothing, Vec::<String>::new());
}

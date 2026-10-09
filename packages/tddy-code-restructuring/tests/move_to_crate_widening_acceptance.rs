//! A cross-crate move widens what the origin still reaches — `#reshape` 7/19 — against a live
//! rust-analyzer and a real toolchain.
//!
//! `move_to_crate_widening.rs` decides what to widen from a reference set a test hands it. This suite
//! proves the half no fake can: that the server's `documentSymbol` tree yields the fields and inherent
//! members, at the positions the edits address; that what lands compiles and lints clean; and that
//! `check --deep` says, before anything is written, every widening `apply` then makes.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use harness::{
    a_cluster_move_of, a_move_of, a_workspace_whose_module_the_origin_still_reaches,
    a_workspace_whose_moved_fn_returns_a_type_no_path_names,
    a_workspace_whose_nested_module_its_parent_globs, applying_keeping_the_account,
    assert_compiles, assert_lints_clean, checking_deep_keeping_the_account, performing,
    resolution_of, source, the_widenings_in, ROSTER_BEFORE, THE_ATTACHMENT_PROGRESS,
    THE_MOVED_ATTACHMENT_PROGRESS, THE_MOVED_FACTORY, THE_MOVED_LEDGER, THE_MOVED_ROSTER,
    THE_ROSTER,
};
use tddy_code_restructuring::{Reexport, VisibilityChange};

/// `roster` as it must land: everything `runtime` and `ledger` name is `pub`, and `secret`,
/// `internal`, `only_the_roster_uses` and the trait `impl` read exactly as written.
fn the_roster_once_widened() -> String {
    source(&[
        "//! The roster.",
        "",
        "pub struct AgentRoster {",
        "    pub rev: u64,",
        "    secret: u64,",
        "}",
        "",
        "impl AgentRoster {",
        "    pub fn new() -> Self {",
        "        Self { rev: 1, secret: 0 }",
        "    }",
        "",
        "    pub fn broadcast(&self) -> u64 {",
        "        self.rev + self.internal()",
        "    }",
        "",
        "    fn internal(&self) -> u64 {",
        "        self.secret + only_the_roster_uses()",
        "    }",
        "}",
        "",
        "impl Default for AgentRoster {",
        "    fn default() -> Self {",
        "        Self::new()",
        "    }",
        "}",
        "",
        "pub fn started_roster_rev(roster: &AgentRoster) -> u64 {",
        "    roster.rev",
        "}",
        "",
        "pub(crate) fn only_the_roster_uses() -> u64 {",
        "    0",
        "}",
    ])
}

fn a_move_of_the_roster() -> tddy_code_restructuring::RefactorOp {
    a_move_of(THE_ROSTER, "roster", Some(Reexport::Glob))
}

fn widened(item: &str) -> VisibilityChange {
    VisibilityChange {
        item: item.to_string(),
        from: "pub(crate)".to_string(),
        to: "pub".to_string(),
        reason: None,
    }
}

/// Test 19 — the `#carve` 21 R3 shape: a module moved behind a facade whose `pub(crate)` type,
/// field, methods and function the origin still uses lands with exactly those widened, and the
/// workspace compiles with no hand edit.
#[tokio::test(flavor = "multi_thread")]
async fn a_module_whose_pub_crate_items_fields_and_methods_the_origin_uses_moves_behind_a_facade_and_compiles(
) {
    // Given a roster that runtime and ledger keep naming
    let workspace = a_workspace_whose_module_the_origin_still_reaches();

    // When it moves to the destination behind a facade
    performing(&workspace, a_move_of_the_roster()).await;

    // Then what the origin names is `pub`, the rest reads as written, and every crate compiles
    assert_eq!(
        workspace.read(THE_MOVED_ROSTER),
        the_roster_once_widened(),
        "the moved roster does not carry exactly the widenings the origin needs"
    );
    assert_compiles(&workspace);
}

/// Test 20 — the 09-25 shape: a nested module its parent re-exports by glob moves with every item
/// the glob made visible widened, including a trait no path names, and every sibling compiles.
#[tokio::test(flavor = "multi_thread")]
async fn a_nested_module_its_parent_re_exports_by_glob_moves_and_every_sibling_still_compiles() {
    // Given `connection_service::attachment_progress`, re-exported by `pub(crate) use …::*;`
    let workspace = a_workspace_whose_nested_module_its_parent_globs();

    // When it moves with no facade
    performing(
        &workspace,
        a_move_of(
            THE_ATTACHMENT_PROGRESS,
            "connection_service::attachment_progress",
            Some(Reexport::None),
        ),
    )
    .await;

    // Then the struct, its field and the trait are `pub`; the trait's items and the private helper
    // read as written; and the sibling still compiles
    assert_eq!(
        workspace.read(THE_MOVED_ATTACHMENT_PROGRESS),
        source(&[
            "pub struct AttachmentProgressSink {",
            "    pub sent: u64,",
            "}",
            "",
            "pub trait Progressing {",
            "    fn tick(&self) -> u64;",
            "}",
            "",
            "impl Progressing for AttachmentProgressSink {",
            "    fn tick(&self) -> u64 {",
            "        self.sent + private_helper()",
            "    }",
            "}",
            "",
            "fn private_helper() -> u64 {",
            "    0",
            "}",
        ]),
        "the moved child does not carry the widenings its parent's glob needs"
    );
    assert_compiles(&workspace);
}

/// Test 21 — a cluster widens, member by member, what the origin still reaches of each, and
/// nothing a co-moving member alone reaches.
#[tokio::test(flavor = "multi_thread")]
async fn a_cluster_move_widens_what_the_origin_reaches_of_every_member_and_compiles() {
    // Given the roster and the ledger, both named by runtime
    let workspace = a_workspace_whose_module_the_origin_still_reaches();

    // When they move together behind a facade
    performing(
        &workspace,
        a_cluster_move_of(&["ledger", "roster"], Some(Reexport::Glob)),
    )
    .await;

    // Then the ledger's function is `pub`, the roster is widened as for a single move, and the
    // workspace compiles
    assert_eq!(
        workspace.read(THE_MOVED_LEDGER),
        source(&[
            "use crate::roster::AgentRoster;",
            "",
            "pub fn ledger_rev(roster: &AgentRoster) -> u64 {",
            "    roster.broadcast()",
            "}",
        ]),
        "the ledger's function the origin calls was not widened"
    );
    assert_eq!(workspace.read(THE_MOVED_ROSTER), the_roster_once_widened());
    assert_compiles(&workspace);
}

/// Test 22 — a type only the signature of a widened function names is widened with it, so the
/// workspace passes clippy with warnings denied (`private_interfaces`).
#[tokio::test(flavor = "multi_thread")]
async fn a_moved_fn_returning_a_type_no_path_names_leaves_the_workspace_clean_under_clippy_with_warnings_denied(
) {
    // Given a factory whose `make` returns a `Widget` nothing outside names
    let workspace = a_workspace_whose_moved_fn_returns_a_type_no_path_names();

    // When it moves behind a facade
    performing(
        &workspace,
        a_move_of(harness::THE_FACTORY, "factory", Some(Reexport::Glob)),
    )
    .await;

    // Then `Widget` is `pub` beside `make` and `size`, and the lint gate is green
    assert_eq!(
        workspace.read(THE_MOVED_FACTORY),
        source(&[
            "//! Makes widgets.",
            "",
            "pub struct Widget {",
            "    size: u64,",
            "}",
            "",
            "impl Widget {",
            "    pub fn size(&self) -> u64 {",
            "        self.size",
            "    }",
            "}",
            "",
            "pub fn make() -> Widget {",
            "    Widget { size: 3 }",
            "}",
        ])
    );
    assert_lints_clean(&workspace);
}

/// Test 23 — the server's outline yields fields and inherent members at the positions the edits
/// address, and the resolution reports each widening, by `Type::member`, in source order.
#[tokio::test(flavor = "multi_thread")]
async fn the_server_reports_fields_and_inherent_methods_at_the_positions_the_edits_address() {
    // Given the roster
    let workspace = a_workspace_whose_module_the_origin_still_reaches();

    // When its move is resolved
    let resolution = resolution_of(&workspace, a_move_of_the_roster())
        .await
        .expect("the move resolves");

    // Then every widening is reported, nothing else is
    assert_eq!(
        resolution.report,
        vec![
            widened("AgentRoster"),
            widened("AgentRoster::rev"),
            widened("AgentRoster::new"),
            widened("AgentRoster::broadcast"),
            widened("started_roster_rev"),
        ]
    );
}

/// Test 24 — `check --deep` prints every widening `apply` then makes, and writes nothing itself.
#[tokio::test(flavor = "multi_thread")]
async fn a_deep_check_prints_every_widening_the_apply_then_makes_and_writes_nothing() {
    // Given the roster's move as a plan
    let workspace = a_workspace_whose_module_the_origin_still_reaches();
    let plan = [a_move_of_the_roster()];

    // When the plan is checked deeply, and then applied
    let (found, checked) = checking_deep_keeping_the_account(&workspace, &plan).await;
    let untouched = workspace.read(THE_ROSTER);
    let (applied, account) = applying_keeping_the_account(&workspace, &plan, false).await;

    // Then the check found nothing, wrote nothing, and said exactly what the apply then did
    assert_eq!(found.expect("the deep check runs"), Vec::<String>::new());
    assert_eq!(
        untouched,
        source(ROSTER_BEFORE),
        "the deep check wrote to the roster"
    );
    applied.expect("the plan applies");
    let expected = [
        "   visibility: `AgentRoster` pub(crate) -> pub",
        "   visibility: `AgentRoster::rev` pub(crate) -> pub",
        "   visibility: `AgentRoster::new` pub(crate) -> pub",
        "   visibility: `AgentRoster::broadcast` pub(crate) -> pub",
        "   visibility: `started_roster_rev` pub(crate) -> pub",
    ];
    assert_eq!(
        the_widenings_in(&checked),
        expected,
        "the deep check's widenings"
    );
    assert_eq!(
        the_widenings_in(&account),
        expected,
        "the apply's widenings"
    );
}

/// Test 25 — a module reached only through its parent's glob has reached items and no caller; the
/// deep check's survey says why, naming the glob.
#[tokio::test(flavor = "multi_thread")]
async fn a_deep_check_explains_a_module_with_reached_items_and_no_callers_by_its_parents_glob() {
    // Given the glob-re-exported child
    let workspace = a_workspace_whose_nested_module_its_parent_globs();

    // When its move is checked deeply
    let (_, account) = checking_deep_keeping_the_account(
        &workspace,
        &[a_move_of(
            THE_ATTACHMENT_PROGRESS,
            "connection_service::attachment_progress",
            Some(Reexport::None),
        )],
    )
    .await;

    // Then the survey names the parent's glob as what reaches it
    assert!(
        account.contains(
            &"      reached through a glob re-export, not by path: \
              crates/origin/src/connection_service.rs:4: pub(crate) use attachment_progress::*;"
                .to_string()
        ),
        "the survey does not name the parent's glob; it said:\n{}",
        account.join("\n")
    );
}

//! Same-crate moves widen what they split apart, and leave no empty directories, against a live
//! rust-analyzer.
//!
//! `move_item` widened the module-level items a move connects, and `reparent_module` the module's
//! own declaration — nothing else. A struct moved away from its `impl`, a method left behind that
//! the moved code calls, or a module that calls its old parent's private helper applied and then
//! stopped at the compile gate (`E0616`, `E0624`, `E0451`, `E0603`), the whole index paid for. And
//! every `git mv` left the directory it emptied on disk.
//!
//! `cargo check --all-targets` and clippy are the assertions no edit that merely looks right can
//! satisfy; the run's own report lines say what was widened, so a reviewer can see it.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use std::sync::Arc;

use harness::{
    a_sink_that_keeps_what_it_hears, applying_the_plan_with, assert_compiles_with_its_tests,
    assert_lints_clean, AFixtureWorkspace,
};
use same_crate::{a_move_item_op, a_reparent_module_op, an_app_holding, the_anchor_over};

const ITEM_LIB: &str = "pub mod answers;\npub mod pairing;\n";
const AN_EMPTY_ANSWERS_MODULE: &str = "//! What a peer answered.\n";
const MODULE_LIB: &str = "pub mod host;\npub mod split;\n";
const AN_IDLE_SPLIT: &str = "pub fn start() -> u32 {\n    0\n}\n";

/// What a run reported, and how it ended.
struct Run {
    outcome: Result<tddy_code_restructuring::runner::RunSummary, String>,
    lines: Vec<String>,
}

impl Run {
    /// Panic with the run's own words unless it applied.
    fn applied(&self) {
        if let Err(said) = &self.outcome {
            panic!("the move did not apply:\n{said}");
        }
    }

    fn said(&self, wanted: &str) -> bool {
        self.lines.iter().any(|line| line.contains(wanted))
    }
}

/// Apply `op` and keep every line the run reports, on its account and its progress alike.
async fn applying_and_listening(
    workspace: &AFixtureWorkspace,
    op: tddy_code_restructuring::RefactorOp,
) -> Run {
    let plan = workspace.a_plan_of(&[op]);
    let (sink, heard) = a_sink_that_keeps_what_it_hears();
    let outcome = applying_the_plan_with(workspace, plan, |options| {
        options.account = Arc::clone(&sink);
        options.progress = sink;
    })
    .await;
    let lines = heard.lock().expect("the lines are readable").clone();
    Run { outcome, lines }
}

/// Move the items `names` of `src/pairing.rs` into the module `to`, re-pointing every caller.
async fn moving_out_of_pairing(workspace: &AFixtureWorkspace, names: &[&str], to: &str) -> Run {
    let anchor = the_anchor_over(workspace, "src/pairing.rs", names).await;
    applying_and_listening(workspace, a_move_item_op(&anchor, to, None)).await
}

/// Re-parent the module `name` that `parent_file` declares under the module `to`.
async fn reparenting(
    workspace: &AFixtureWorkspace,
    parent_file: &str,
    name: &str,
    to: &str,
) -> Run {
    let anchor = the_anchor_over(workspace, parent_file, &[name]).await;
    applying_and_listening(workspace, a_reparent_module_op(&anchor, to, None)).await
}

/// `pairing` holding `text`, beside an empty `answers`.
fn a_pairing_module_reading(text: &str) -> AFixtureWorkspace {
    an_app_holding(&[
        ("src/lib.rs", ITEM_LIB),
        ("src/answers.rs", AN_EMPTY_ANSWERS_MODULE),
        ("src/pairing.rs", text),
    ])
}

const A_COUNTER_AND_ITS_IMPL: &str = concat!(
    "pub struct Counter {\n",
    "    count: u32,\n",
    "}\n",
    "\n",
    "impl Counter {\n",
    "    pub fn bump(&mut self) -> u32 {\n",
    "        self.count += 1;\n",
    "        self.count\n",
    "    }\n",
    "}\n",
);

const A_COUNTER_WITH_A_PRIVATE_PEEK_AND_ITS_TALLY: &str = concat!(
    "pub struct Counter {\n",
    "    pub count: u32,\n",
    "}\n",
    "\n",
    "impl Counter {\n",
    "    fn peek(&self) -> u32 {\n",
    "        self.count\n",
    "    }\n",
    "}\n",
    "\n",
    "pub fn tally(counter: &Counter) -> u32 {\n",
    "    counter.peek() + 1\n",
    "}\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn widens_a_private_field_of_a_moved_struct_that_the_impl_left_behind_reads() {
    // Given `Counter` with a private field, and an `impl` that reads it
    let workspace = a_pairing_module_reading(A_COUNTER_AND_ITS_IMPL);

    // When the struct alone moves into `answers`
    let run = moving_out_of_pairing(&workspace, &["Counter"], "app::answers").await;

    // Then the field is visible to the crate, no wider, and the run says so
    run.applied();
    let arrived = workspace.read("src/answers.rs");
    assert!(
        arrived.contains("    pub(crate) count: u32,"),
        "the field the `impl` left behind reads was not widened:\n{arrived}"
    );
    assert!(
        run.said("`Counter::count` private -> pub(crate)"),
        "the widening was not reported: {:?}",
        run.lines
    );
    assert_compiles_with_its_tests(&workspace);
    assert_lints_clean(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn widens_a_private_field_of_a_struct_that_stays_when_its_impl_moves() {
    // Given `Counter` with a private field, and an `impl` that reads it
    let workspace = a_pairing_module_reading(A_COUNTER_AND_ITS_IMPL);

    // When the `impl` alone moves into `answers`
    let run = moving_out_of_pairing(&workspace, &["<Counter>"], "app::answers").await;

    // Then the field is widened where it stays
    run.applied();
    let stayed = workspace.read("src/pairing.rs");
    assert!(
        stayed.contains("    pub(crate) count: u32,"),
        "the field the moved `impl` reads was not widened:\n{stayed}"
    );
    assert!(
        run.said("`Counter::count` private -> pub(crate)"),
        "the widening was not reported: {:?}",
        run.lines
    );
    assert_compiles_with_its_tests(&workspace);
    assert_lints_clean(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn widens_a_private_method_that_stays_when_the_moved_code_calls_it() {
    // Given a private method of `Counter`, and a function that calls it
    let workspace = a_pairing_module_reading(A_COUNTER_WITH_A_PRIVATE_PEEK_AND_ITS_TALLY);

    // When the function moves into `answers`
    let run = moving_out_of_pairing(&workspace, &["tally"], "app::answers").await;

    // Then the method is widened where it stays
    run.applied();
    let stayed = workspace.read("src/pairing.rs");
    assert!(
        stayed.contains("    pub(crate) fn peek(&self) -> u32 {"),
        "the method the moved code calls was not widened:\n{stayed}"
    );
    assert!(
        run.said("`Counter::peek` private -> pub(crate)"),
        "the widening was not reported: {:?}",
        run.lines
    );
    assert_compiles_with_its_tests(&workspace);
    assert_lints_clean(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn widens_a_private_method_of_a_moved_impl_that_the_code_left_behind_calls() {
    // Given a private method of `Counter`, and a function that calls it
    let workspace = a_pairing_module_reading(A_COUNTER_WITH_A_PRIVATE_PEEK_AND_ITS_TALLY);

    // When the struct and its `impl` move into `answers`, and the function stays
    let run = moving_out_of_pairing(&workspace, &["Counter", "<Counter>"], "app::answers").await;

    // Then the method is widened where it lands
    run.applied();
    let arrived = workspace.read("src/answers.rs");
    assert!(
        arrived.contains("    pub(crate) fn peek(&self) -> u32 {"),
        "the method the code left behind calls was not widened:\n{arrived}"
    );
    assert!(
        run.said("`Counter::peek` private -> pub(crate)"),
        "the widening was not reported: {:?}",
        run.lines
    );
    assert_compiles_with_its_tests(&workspace);
    assert_lints_clean(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn widens_a_private_field_that_a_function_left_behind_builds_with_a_struct_literal() {
    // Given a function that builds `Counter` by naming its private field
    let workspace = a_pairing_module_reading(concat!(
        "pub struct Counter {\n",
        "    count: u32,\n",
        "}\n",
        "\n",
        "impl Counter {\n",
        "    pub fn count(&self) -> u32 {\n",
        "        self.count\n",
        "    }\n",
        "}\n",
        "\n",
        "pub fn fresh() -> Counter {\n",
        "    Counter { count: 0 }\n",
        "}\n",
    ));

    // When the struct and its `impl` move into `answers`, and the function stays
    let run = moving_out_of_pairing(&workspace, &["Counter", "<Counter>"], "app::answers").await;

    // Then the field is widened where it lands, and the public method of the same name is not
    run.applied();
    let arrived = workspace.read("src/answers.rs");
    assert!(
        arrived.contains("    pub(crate) count: u32,"),
        "the field the struct literal names was not widened:\n{arrived}"
    );
    assert!(
        arrived.contains("    pub fn count(&self) -> u32 {"),
        "the public method sharing the field's name was changed:\n{arrived}"
    );
    assert_compiles_with_its_tests(&workspace);
    assert_lints_clean(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn leaves_every_member_private_when_the_destination_is_a_child_of_the_source() {
    // Given `Counter` and its `impl` in `pairing`, which has a child `inner`
    let workspace = an_app_holding(&[
        ("src/lib.rs", ITEM_LIB),
        ("src/answers.rs", AN_EMPTY_ANSWERS_MODULE),
        (
            "src/pairing.rs",
            &format!("pub mod inner;\n\n{A_COUNTER_AND_ITS_IMPL}"),
        ),
        ("src/pairing/inner.rs", "//! Inside pairing.\n"),
    ]);

    // When the `impl` moves into the child
    let run = moving_out_of_pairing(&workspace, &["<Counter>"], "app::pairing::inner").await;

    // Then the child still sees its parent's private field: the member was surveyed, and nothing
    // was widened
    run.applied();
    assert!(
        run.said("surveying 1 member(s) of the types the move splits"),
        "the move did not survey the field the moved `impl` reads: {:?}",
        run.lines
    );
    let stayed = workspace.read("src/pairing.rs");
    assert!(
        stayed.contains("    count: u32,") && !stayed.contains("pub(crate) count"),
        "a field a child can see was widened:\n{stayed}"
    );
    assert!(
        !run.said("`Counter::count`"),
        "a widening was reported for a field nobody needed widened: {:?}",
        run.lines
    );
    assert_compiles_with_its_tests(&workspace);
    assert_lints_clean(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn never_touches_the_members_of_a_trait_impl() {
    // Given a `Display` impl of `Counter` that reads its private field
    const THE_FMT_LINE: &str =
        "    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n";
    let workspace = a_pairing_module_reading(&format!(
        concat!(
            "use std::fmt::Display;\n",
            "\n",
            "pub struct Counter {{\n",
            "    count: u32,\n",
            "}}\n",
            "\n",
            "impl Display for Counter {{\n",
            "{}",
            "        write!(f, \"{{}}\", self.count)\n",
            "    }}\n",
            "}}\n",
        ),
        THE_FMT_LINE
    ));

    // When the trait `impl` moves into `answers`
    let run = moving_out_of_pairing(&workspace, &["<Counter as Display>"], "app::answers").await;

    // Then only the field is widened; the trait method arrives exactly as it was written
    run.applied();
    assert!(
        workspace
            .read("src/pairing.rs")
            .contains("    pub(crate) count: u32,"),
        "the field the trait `impl` reads was not widened:\n{}",
        workspace.read("src/pairing.rs")
    );
    assert!(
        workspace.read("src/answers.rs").contains(THE_FMT_LINE),
        "a trait method was given a visibility:\n{}",
        workspace.read("src/answers.rs")
    );
    assert_compiles_with_its_tests(&workspace);
    assert_lints_clean(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn surveys_only_the_members_the_moved_lines_mention() {
    // Given a struct with five private fields, of which the function to move reads one
    let workspace = a_pairing_module_reading(concat!(
        "pub struct Counter {\n",
        "    first: u32,\n",
        "    second: u32,\n",
        "    third: u32,\n",
        "    fourth: u32,\n",
        "    fifth: u32,\n",
        "}\n",
        "\n",
        "impl Counter {\n",
        "    pub fn total(&self) -> u32 {\n",
        "        self.first + self.second + self.third + self.fourth + self.fifth\n",
        "    }\n",
        "}\n",
        "\n",
        "pub fn first_of(counter: &Counter) -> u32 {\n",
        "    counter.first\n",
        "}\n",
    ));

    // When the function moves into `answers`
    let run = moving_out_of_pairing(&workspace, &["first_of"], "app::answers").await;

    // Then one field is surveyed and widened, and the four it never names are left as written
    run.applied();
    assert!(
        run.said("surveying 1 member(s) of the types the move splits"),
        "the survey did not stop at the members the moved lines mention: {:?}",
        run.lines
    );
    let stayed = workspace.read("src/pairing.rs");
    assert!(
        stayed.contains("    pub(crate) first: u32,") && stayed.contains("    second: u32,\n"),
        "the fields were not widened exactly as far as the moved code needs:\n{stayed}"
    );
    assert_compiles_with_its_tests(&workspace);
    assert_lints_clean(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn widens_a_private_item_of_the_old_parent_that_the_moved_tree_names() {
    // Given `attachments` calling its parent's private `host_name`
    let workspace = an_app_holding(&[
        ("src/lib.rs", MODULE_LIB),
        (
            "src/host.rs",
            "pub mod attachments;\n\nfn host_name() -> &'static str {\n    \"host\"\n}\n",
        ),
        (
            "src/host/attachments.rs",
            "pub fn materialize() -> u32 {\n    super::host_name().len() as u32\n}\n",
        ),
        ("src/split.rs", AN_IDLE_SPLIT),
    ]);

    // When `attachments` is re-parented under `split`
    let run = reparenting(&workspace, "src/host.rs", "attachments", "app::split").await;

    // Then `host_name` is visible to the crate, no wider, and the run says so
    run.applied();
    let host = workspace.read("src/host.rs");
    assert!(
        host.contains("pub(crate) fn host_name() -> &'static str {"),
        "the old parent's private item the tree calls was not widened:\n{host}"
    );
    assert!(
        run.said("`host_name` private -> pub(crate)"),
        "the widening was not reported: {:?}",
        run.lines
    );
    assert_compiles_with_its_tests(&workspace);
    assert_lints_clean(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn widens_a_grandparents_private_item_but_not_one_of_an_ancestor_both_places_share() {
    // Given `a::b::host::attachments` calling a private item of `a::b` and one of `a`
    let workspace = an_app_holding(&[
        ("src/lib.rs", "pub mod a;\n"),
        (
            "src/a.rs",
            "pub mod b;\npub mod split;\n\nfn shared() -> u32 {\n    1\n}\n",
        ),
        (
            "src/a/b.rs",
            "pub mod host;\n\nfn helper() -> u32 {\n    2\n}\n",
        ),
        ("src/a/b/host.rs", "pub mod attachments;\n"),
        (
            "src/a/b/host/attachments.rs",
            "pub fn materialize() -> u32 {\n    super::super::helper() + super::super::super::shared()\n}\n",
        ),
        ("src/a/split.rs", AN_IDLE_SPLIT),
    ]);

    // When `attachments` is re-parented under `a::split`
    let run = reparenting(
        &workspace,
        "src/a/b/host.rs",
        "attachments",
        "app::a::split",
    )
    .await;

    // Then `a::b`'s helper is widened to `a`, and `a`'s own item, which both places sit under, is
    // left as written
    run.applied();
    assert!(
        workspace
            .read("src/a/b.rs")
            .contains("pub(super) fn helper() -> u32 {"),
        "the grandparent's private item was not widened to the shared ancestor:\n{}",
        workspace.read("src/a/b.rs")
    );
    assert!(
        workspace
            .read("src/a.rs")
            .contains("\nfn shared() -> u32 {"),
        "an item of an ancestor both places share was widened:\n{}",
        workspace.read("src/a.rs")
    );
    assert_compiles_with_its_tests(&workspace);
    assert_lints_clean(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn widens_a_pub_super_item_of_the_moved_module_that_its_old_parent_names() {
    // Given `attachments::materialize`, visible to its parent only, which the parent calls
    let workspace = an_app_holding(&[
        ("src/lib.rs", MODULE_LIB),
        (
            "src/host.rs",
            "pub mod attachments;\n\npub fn host_name() -> u32 {\n    attachments::materialize()\n}\n",
        ),
        (
            "src/host/attachments.rs",
            "pub(super) fn materialize() -> u32 {\n    1\n}\n",
        ),
        ("src/split.rs", AN_IDLE_SPLIT),
    ]);

    // When `attachments` is re-parented under `split`
    let run = reparenting(&workspace, "src/host.rs", "attachments", "app::split").await;

    // Then the old parent still sees it, through a visibility that is legal where it now sits
    run.applied();
    let moved = workspace.read("src/split/attachments.rs");
    assert!(
        moved.contains("pub(crate) fn materialize() -> u32 {"),
        "the `pub(super)` item its old parent calls was not widened:\n{moved}"
    );
    assert!(
        run.said("`materialize` pub(super) -> pub(crate)"),
        "the widening was not reported: {:?}",
        run.lines
    );
    assert_compiles_with_its_tests(&workspace);
    assert_lints_clean(&workspace);
}

/// `host::attachments` with a child `staging` whose items are visible by an absolute path into the
/// tree and by a relative one.
fn a_tree_with_absolute_and_relative_visibilities() -> AFixtureWorkspace {
    an_app_holding(&[
        ("src/lib.rs", MODULE_LIB),
        ("src/host.rs", "pub mod attachments;\n"),
        (
            "src/host/attachments.rs",
            "pub mod staging;\n\npub fn materialize() -> u32 {\n    staging::stage() + staging::count()\n}\n",
        ),
        (
            "src/host/attachments/staging.rs",
            concat!(
                "pub(in crate::host::attachments) fn stage() -> u32 {\n",
                "    2\n",
                "}\n",
                "\n",
                "pub(super) fn count() -> u32 {\n",
                "    3\n",
                "}\n",
            ),
        ),
        ("src/split.rs", AN_IDLE_SPLIT),
    ])
}

#[tokio::test(flavor = "multi_thread")]
async fn respells_an_absolute_pub_in_path_that_points_into_the_moved_tree() {
    // Given a child item visible by the absolute path of the module that moves
    let workspace = a_tree_with_absolute_and_relative_visibilities();

    // When `attachments` is re-parented under `split`
    let run = reparenting(&workspace, "src/host.rs", "attachments", "app::split").await;

    // Then the path follows the tree
    run.applied();
    let staging = workspace.read("src/split/attachments/staging.rs");
    assert!(
        staging.contains("pub(in crate::split::attachments) fn stage() -> u32 {"),
        "the absolute visibility still names the old place:\n{staging}"
    );
    assert_compiles_with_its_tests(&workspace);
    assert_lints_clean(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn leaves_a_pub_super_inside_the_moved_tree_as_written() {
    // Given a child item visible to its parent, which moves with it
    let workspace = a_tree_with_absolute_and_relative_visibilities();

    // When `attachments` is re-parented under `split`
    let run = reparenting(&workspace, "src/host.rs", "attachments", "app::split").await;

    // Then the relative visibility means the same thing where the tree sits now, and is untouched
    run.applied();
    let staging = workspace.read("src/split/attachments/staging.rs");
    assert!(
        staging.contains("\npub(super) fn count() -> u32 {"),
        "a visibility inside the moved tree was respelled:\n{staging}"
    );
    assert!(
        !run.said("`count`"),
        "a visibility that kept its meaning was reported: {:?}",
        run.lines
    );
    assert_compiles_with_its_tests(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn removes_the_directories_the_move_emptied() {
    // Given `host`'s only child, `attachments`, with a child of its own
    let workspace = an_app_holding(&[
        ("src/lib.rs", MODULE_LIB),
        ("src/host.rs", "pub mod attachments;\n"),
        (
            "src/host/attachments.rs",
            "pub mod staging;\n\npub fn materialize() -> u32 {\n    staging::stage()\n}\n",
        ),
        (
            "src/host/attachments/staging.rs",
            "pub fn stage() -> u32 {\n    2\n}\n",
        ),
        ("src/split.rs", AN_IDLE_SPLIT),
    ]);

    // When `attachments` is re-parented under `split`
    let run = reparenting(&workspace, "src/host.rs", "attachments", "app::split").await;

    // Then no directory it emptied is left, and the source directory is kept
    run.applied();
    assert!(
        !workspace.path().join("src/host/attachments").exists(),
        "the moved module's directory was left on disk"
    );
    assert!(
        !workspace.path().join("src/host").exists(),
        "the old parent's emptied directory was left on disk"
    );
    assert!(workspace.path().join("src").is_dir());
    assert_compiles_with_its_tests(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn keeps_a_directory_that_still_holds_a_file_no_declaration_reaches() {
    // Given `attachments` holding a data file no `mod` reaches, beside a child with a child
    let workspace = an_app_holding(&[
        ("src/lib.rs", MODULE_LIB),
        ("src/host.rs", "pub mod attachments;\n"),
        (
            "src/host/attachments.rs",
            "pub mod staging;\n\npub fn materialize() -> u32 {\n    staging::stage()\n}\n",
        ),
        (
            "src/host/attachments/staging.rs",
            "pub mod deep;\n\npub fn stage() -> u32 {\n    deep::level()\n}\n",
        ),
        (
            "src/host/attachments/staging/deep.rs",
            "pub fn level() -> u32 {\n    3\n}\n",
        ),
        ("src/host/attachments/fixture.txt", "data\n"),
        ("src/split.rs", AN_IDLE_SPLIT),
    ]);

    // When `attachments` is re-parented under `split`
    let run = reparenting(&workspace, "src/host.rs", "attachments", "app::split").await;

    // Then the emptied child directory is gone, and the one still holding the data file stays
    run.applied();
    assert!(
        !workspace
            .path()
            .join("src/host/attachments/staging")
            .exists(),
        "the emptied child directory was left on disk"
    );
    assert!(
        workspace.holds("src/host/attachments/fixture.txt"),
        "a file the move does not own was removed"
    );
    assert_compiles_with_its_tests(&workspace);
}

//! `move_module_to_crate` against a live rust-analyzer and a real toolchain.
//!
//! The unit suites decide what to write from a reference set a test hands them. These two prove the
//! other half: that the reference set the *server* returns drives the same decisions, and that the
//! result compiles. `cargo check` is the assertion that cannot be satisfied by an edit which merely
//! looks right — a caller left naming a crate nobody depends on fails it, and did.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use harness::{
    a_move_of, a_move_of_the_host_registry, a_workspace_a_module_can_move_across,
    a_workspace_whose_module_has_a_directory_child,
    a_workspace_whose_module_has_a_sibling_test_module, applying_keeping_the_account,
    assert_compiles, assert_compiles_with_its_tests, performing, A_DIRECTORY_CHILD,
    A_SIBLING_TEST_MODULE,
};
use tddy_code_restructuring::Reexport;

const MODULE: &str = "crates/origin/src/host_registry.rs";
const MOVED_TO: &str = "crates/destination/src/host_registry.rs";
const CALLER: &str = "crates/origin/src/runtime.rs";

/// The whole operation, end to end: the file moves, the caller the server found is re-pointed, both
/// manifests gain what the new shape needs, and the workspace still compiles.
#[tokio::test(flavor = "multi_thread")]
async fn relocates_a_module_and_leaves_every_crate_compiling() {
    // Given
    let workspace = a_workspace_a_module_can_move_across();

    // When
    performing(&workspace, a_move_of_the_host_registry(None)).await;

    // Then
    assert!(workspace.holds(MOVED_TO), "the module file did not arrive");
    assert!(!workspace.holds(MODULE), "the module file did not leave");
    assert_eq!(
        workspace.read(CALLER),
        "use destination::host_registry::HostRegistry;\n\npub fn boot() -> u64 {\n    \
         HostRegistry::new().stamp()\n}\n",
        "the caller rust-analyzer reported was not re-pointed"
    );
    assert_compiles(&workspace);
}

/// The move a reviewer can read: the crate the module left re-exports it, so every caller keeps
/// resolving through the path it already writes and not one of them is rewritten.
#[tokio::test(flavor = "multi_thread")]
async fn leaves_every_caller_untouched_when_a_facade_is_left_behind() {
    // Given
    let workspace = a_workspace_a_module_can_move_across();
    let before = workspace.read(CALLER);

    // When
    performing(
        &workspace,
        a_move_of_the_host_registry(Some(Reexport::Glob)),
    )
    .await;

    // Then
    assert!(workspace.holds(MOVED_TO), "the module file did not arrive");
    assert_eq!(
        workspace.read(CALLER),
        before,
        "a faceded move rewrote a caller"
    );
    assert_compiles(&workspace);
}

/// The file count an operation's account line reports — `[1/1] op 0: … -> N file(s) applied`.
fn files_reported(account: &[String]) -> Vec<String> {
    account
        .iter()
        .filter_map(|line| line.split_once(" -> "))
        .filter_map(|(_, rest)| rest.split_once(" file(s)"))
        .map(|(count, _)| count.to_string())
        .collect()
}

/// `#reshape` 5/19 — a module in the `foo.rs` + `foo/` shape moves with its directory child, the
/// dry run and the apply report the same file count and name the files that move, and every crate
/// compiles with its tests. `#live-plan` 12/15 stranded the children here (`E0583`).
#[tokio::test(flavor = "multi_thread")]
async fn relocates_a_module_with_its_directory_children_and_every_crate_compiles_with_its_tests() {
    // Given `host_registry`, whose child `clock_face` lives in `host_registry/`
    let workspace = a_workspace_whose_module_has_a_directory_child();
    let plan = [a_move_of(MODULE, "host_registry", None)];

    // When the plan is dry-run, then applied
    let (rehearsed, rehearsal) = applying_keeping_the_account(&workspace, &plan, true).await;
    let (applied, account) = applying_keeping_the_account(&workspace, &plan, false).await;

    // Then both runs succeed and agree on the count, both name what moves with the module, and
    // the child arrived beside its module
    let moves_with_it = "move with `host_registry`, with the directory of its children";
    assert_eq!(
        (
            rehearsed.map(|run| run.applied),
            applied.map(|run| run.applied),
            files_reported(&rehearsal) == files_reported(&account),
            rehearsal.iter().any(|line| line.contains(moves_with_it)),
            account.iter().any(|line| line.contains(moves_with_it)),
            workspace.holds("crates/destination/src/host_registry/clock_face.rs"),
            workspace.holds(A_DIRECTORY_CHILD),
        ),
        (Ok(1), Ok(1), true, true, true, true, false),
        "dry run: {rehearsal:?}\napply: {account:?}"
    );
    assert_compiles_with_its_tests(&workspace);
}

/// Test 26 (`#reshape` 9) — a move whose line carries `name` creates the crate it moves into: the
/// manifest, the root and the workspace entry are part of the same edit, and cargo builds the
/// result. `#carve` built every such skeleton by hand, because a `to` with no `Cargo.toml` was
/// refused.
#[tokio::test(flavor = "multi_thread")]
async fn relocates_a_module_into_a_crate_the_move_creates_and_leaves_every_crate_compiling() {
    // Given a module and a destination directory that holds no crate yet
    let workspace = a_workspace_a_module_can_move_across();
    let mut creating = a_move_of_the_host_registry(None);
    creating.to = Some("crates/fresh".to_string());
    creating.name = Some("fresh".to_string());

    // When the move creating `fresh` is performed
    performing(&workspace, creating).await;

    // Then the crate exists, the workspace lists it, the caller names it, and everything compiles
    assert!(
        workspace.holds("crates/fresh/Cargo.toml"),
        "no manifest was created"
    );
    assert!(
        workspace.holds("crates/fresh/src/host_registry.rs"),
        "the module file did not arrive"
    );
    assert_eq!(
        workspace.read("crates/fresh/src/lib.rs"),
        "pub mod host_registry;\n"
    );
    assert!(
        workspace.read("Cargo.toml").contains("\"crates/fresh\","),
        "the workspace does not list the new crate: {}",
        workspace.read("Cargo.toml")
    );
    assert_eq!(
        workspace.read(CALLER),
        "use fresh::host_registry::HostRegistry;\n\npub fn boot() -> u64 {\n    \
         HostRegistry::new().stamp()\n}\n",
        "the caller was not re-pointed at the new crate"
    );
    assert_compiles(&workspace);
}

/// Test 22 (`#reshape` 14/19) — a test module declared beside the moved module, naming nothing else
/// in the origin, moves with it: its file lands beside the module, its declaration moves from the
/// origin's root to the destination's, the dry run and the apply agree on the count and both say it
/// follows, and every crate compiles with its tests. `#carve` 21/21 R9 moved three such files by hand.
#[tokio::test(flavor = "multi_thread")]
async fn moves_a_module_with_its_sibling_test_module_and_every_crate_compiles_with_its_tests() {
    // Given `host_registry`, beside which the crate root declares `host_registry_tests`
    let workspace = a_workspace_whose_module_has_a_sibling_test_module();
    let plan = [a_move_of(MODULE, "host_registry", Some(Reexport::Glob))];

    // When the plan is dry-run, then applied
    let (rehearsed, rehearsal) = applying_keeping_the_account(&workspace, &plan, true).await;
    let (applied, account) = applying_keeping_the_account(&workspace, &plan, false).await;

    // Then both runs agree and name the follower, the test file sits beside its module, and the
    // declaration went with it
    let follows = "test module `host_registry_tests`";
    assert_eq!(
        (
            rehearsed.map(|run| run.applied),
            applied.map(|run| run.applied),
            files_reported(&rehearsal) == files_reported(&account),
            rehearsal.iter().any(|line| line.contains(follows)),
            account.iter().any(|line| line.contains(follows)),
            workspace.holds("crates/destination/src/host_registry_tests.rs"),
            workspace.holds(A_SIBLING_TEST_MODULE),
            workspace.read("crates/destination/src/lib.rs").contains(
                "/// Tests the registry alone.\n#[cfg(test)]\nmod host_registry_tests;\n"
            ),
            workspace
                .read("crates/origin/src/lib.rs")
                .contains("mod host_registry_tests;"),
        ),
        (Ok(1), Ok(1), true, true, true, true, false, true, false),
        "dry run: {rehearsal:?}\napply: {account:?}"
    );
    assert_compiles_with_its_tests(&workspace);
}

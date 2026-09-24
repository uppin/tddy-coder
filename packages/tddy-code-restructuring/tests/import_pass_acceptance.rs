//! The import pass behind `extract_module`, against a live rust-analyzer.
//!
//! These cover six things: defect E1, the D8 alias restoration E1 sits in the path of, the
//! restoration of a name the parent lost to the seam, which E1's fix must not scope away, the
//! grouped-`use` ambiguity, the rebasing of a relative `use` for the deeper module, and what the
//! pass does on a server that has only just started. Each seam is judged by what `cargo check` says
//! of the tree afterwards. The engine can report success over a `use` line that resolves nothing, so
//! the compiler is the only check it cannot fool.
//!
//! Most of these run against a **settled** server, the warm index the destructure plans were
//! checked against. A server that has only just started reports no unresolved names for a few
//! seconds, so on it the import pass has nothing to act on, and none of these defects appear. The
//! last test here is about that window itself.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use std::ops::RangeInclusive;

use tddy_code_restructuring::{Reexport, RefactorOp};

use harness::{
    a_crate_whose_alias_only_the_compiler_resolves, a_crate_whose_alias_the_server_resolves,
    a_crate_whose_module_aliases_its_parent_s_type_through_super,
    a_crate_whose_module_imports_a_type_only_the_seam_names_through_super,
    a_crate_whose_module_imports_its_parent_s_type_through_super,
    a_crate_whose_parent_names_a_trait_the_seam_moves,
    a_workspace_whose_parent_binds_a_module_in_a_group, an_extract_module_of, assert_compiles,
    assert_compiles_with_its_tests, performing, performing_once_settled, refusal_once_settled_from,
    the_module_named, HOST_MODULE, ORIGIN_LIB, TALLYING_MODULE,
};

/// `fn constant() -> u32 { 7 }`: a seam that names nothing at all.
const A_SEAM_NAMING_NOTHING: RangeInclusive<u32> = 17..=19;
/// `fn second(kind: &StartSessionEventKind)`: a seam whose moved code names the parent's alias.
const A_SEAM_NAMING_THE_ALIAS: RangeInclusive<u32> = 21..=23;
/// `fn open_channel() -> mpsc::Sender`: the only use of the parent's grouped `mpsc`.
const A_SEAM_HOLDING_THE_ONLY_USE_OF_MPSC: RangeInclusive<u32> = 11..=13;
/// `pub trait Named { … }`, which the parent goes on naming in an `impl` and a `&dyn`.
const A_SEAM_TAKING_THE_TRAIT: RangeInclusive<u32> = 3..=5;

/// `fn tally(&self, failure: Failure)` in `host.rs`: a seam naming the type `host` imports through
/// `super`.
const A_SEAM_NAMING_THE_PARENT_S_TYPE: RangeInclusive<u32> = 12..=17;

/// `fn tally(&self, level: u32, failure: Failure)` in `host.rs`: the file's only use of `Failure`,
/// naming it three times.
const A_SEAM_HOLDING_EVERY_USE_OF_THE_PARENT_S_TYPE: RangeInclusive<u32> = 14..=23;

const THE_PARENT_S_ALIAS: &str = "use crate::proto::Event as StartSessionEventKind;";

/// E1: a seam that names nothing needs no import, whatever the rest of the file cannot resolve.
///
/// The alias is unresolved everywhere the parent uses it, and none of those places moves. Today the
/// pass collects unresolved names from the whole file, rebuilds the parent's `use … as …` for the
/// alias and writes it into the new module. The name stays unresolved and the line is written again,
/// until the 512-pass backstop fails the operation. That is every seam of `connection_service.rs`.
#[tokio::test(flavor = "multi_thread")]
async fn extracts_a_seam_naming_nothing_from_a_file_whose_alias_the_server_cannot_resolve() {
    // Given
    let workspace = a_crate_whose_alias_only_the_compiler_resolves();
    let seam = an_extract_module_of(&workspace, ORIGIN_LIB, A_SEAM_NAMING_NOTHING, "constants");

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let module = the_module_named(&workspace.read(ORIGIN_LIB), "constants");
    assert!(
        !module.contains("use "),
        "a seam that names nothing was given imports:\n{module}"
    );
    assert_compiles(&workspace);
}

/// E1: an alias the rebuilt `use` cannot resolve is refused by name, in a bounded number of passes.
///
/// The moved code does name the alias here, and writing the parent's `use … as …` into the module
/// leaves it exactly as unresolved as before. An import that makes no progress has to mark the name
/// unimportable and end the pass. Today it is written again on every pass, and the operation fails
/// only at the backstop, as a server defect that names nothing the plan's author can act on.
#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_seam_naming_an_alias_the_server_cannot_resolve_and_says_which() {
    // Given
    let workspace = a_crate_whose_alias_only_the_compiler_resolves();
    let seam = an_extract_module_of(&workspace, ORIGIN_LIB, A_SEAM_NAMING_THE_ALIAS, "readings");

    // When
    let refusal = refusal_once_settled_from(&workspace, seam).await;

    // Then
    assert!(
        refusal.starts_with("this seam cannot be cut here"),
        "an import that could not make progress was not refused as a seam: {refusal}"
    );
    assert!(
        refusal.contains("`StartSessionEventKind`"),
        "the refusal did not name the alias it could not import: {refusal}"
    );
}

/// The D8 guard: the moved code keeps the alias it was written against, and gets it once.
///
/// rust-analyzer offers only the *unaliased* path, which does not bind `StartSessionEventKind`, so
/// the pass rebuilds the parent's own declaration. Whatever fixes E1 must not stop it doing that.
#[tokio::test(flavor = "multi_thread")]
async fn imports_the_alias_the_moved_code_names_exactly_once() {
    // Given
    let workspace = a_crate_whose_alias_the_server_resolves();
    let seam = an_extract_module_of(&workspace, ORIGIN_LIB, A_SEAM_NAMING_THE_ALIAS, "readings");

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let module = the_module_named(&workspace.read(ORIGIN_LIB), "readings");
    assert_eq!(
        module.matches(THE_PARENT_S_ALIAS).count(),
        1,
        "the module did not import the parent's alias exactly once:\n{module}"
    );
    assert_compiles(&workspace);
}

/// A name the parent bound in a grouped `use` is the file's own binding, even once the assist has
/// taken it out of the group.
///
/// `mpsc` could come from `shared::sync` or from `std::sync`. The parent said which one, in
/// `use shared::sync::{mpsc, RwLock};`. But the seam holds the only use, so the assist drops `mpsc`
/// from that group before the import pass asks. The file's remaining imports then point at both
/// candidate modules (`std::sync::Arc` and `shared::sync::RwLock`), and the seam is refused as
/// "could be imported 2 ways". That is `cli_session_manager.rs` and `tokio::sync::{…, mpsc, …}`.
#[tokio::test(flavor = "multi_thread")]
async fn imports_the_module_the_parent_bound_in_a_group_the_seam_empties() {
    // Given
    let workspace = a_workspace_whose_parent_binds_a_module_in_a_group();
    let seam = an_extract_module_of(
        &workspace,
        ORIGIN_LIB,
        A_SEAM_HOLDING_THE_ONLY_USE_OF_MPSC,
        "channels",
    );

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let module = the_module_named(&workspace.read(ORIGIN_LIB), "channels");
    assert!(
        module.contains("use shared::sync::mpsc;"),
        "the module did not import the `mpsc` the parent had bound:\n{module}"
    );
    assert_compiles(&workspace);
}

/// The guard on how narrowly E1's fix scopes the pass: a name the *parent* lost to the seam is
/// still restored.
///
/// E1 is fixed by ignoring what the file could not resolve before the cut. What the cut itself
/// strands in the parent is a different thing: the assist leaves `impl Named for Thing` naming a
/// trait that is no longer in scope, and only a `use` in the parent brings it back.
#[tokio::test(flavor = "multi_thread")]
async fn imports_into_the_parent_a_trait_the_seam_moved_out_from_under_it() {
    // Given
    let workspace = a_crate_whose_parent_names_a_trait_the_seam_moves();
    let seam = an_extract_module_of(&workspace, ORIGIN_LIB, A_SEAM_TAKING_THE_TRAIT, "naming");

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("use crate::naming::Named;"),
        "the parent was not given back the trait it still names:\n{lib}"
    );
    assert_compiles(&workspace);
}

/// A relative `use` the parent wrote is rebased for the module the seam becomes, which is one level
/// deeper.
///
/// `host.rs` binds `use super::Failure;`. Written verbatim into `mod tallying` inside `host`, that
/// `super` is `host`, not `service`. That is plan 05a on `svc_spawn_split_agent.rs` and its
/// `use super::SplitStartFailure;`.
///
/// A guard, not a reproduction: here rust-analyzer offers `super::super::Failure` itself, so the
/// pass never falls back to the parent's declaration. The alias test below is the one that goes
/// through a reconstruction.
#[tokio::test(flavor = "multi_thread")]
async fn imports_the_parent_s_type_the_moved_code_names_through_super() {
    // Given
    let workspace = a_crate_whose_module_imports_its_parent_s_type_through_super();
    let seam = an_extract_module_of(
        &workspace,
        HOST_MODULE,
        A_SEAM_NAMING_THE_PARENT_S_TYPE,
        "tallying",
    );

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let module = the_module_named(&workspace.read(HOST_MODULE), "tallying");
    assert!(
        module.contains("use super::super::Failure;"),
        "the module was not given the parent's `super::Failure`, one level deeper:\n{module}"
    );
    assert_compiles(&workspace);
}

/// The same, where the parent binds its parent's type under an alias:
/// `use super::Failure as HostFailure;`. The alias reconstruction is rebased the same way.
#[tokio::test(flavor = "multi_thread")]
async fn imports_the_parent_s_alias_of_a_type_it_reaches_through_super() {
    // Given
    let workspace = a_crate_whose_module_aliases_its_parent_s_type_through_super();
    let seam = an_extract_module_of(
        &workspace,
        HOST_MODULE,
        A_SEAM_NAMING_THE_PARENT_S_TYPE,
        "tallying",
    );

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let module = the_module_named(&workspace.read(HOST_MODULE), "tallying");
    assert!(
        module.contains("use super::super::Failure as HostFailure;"),
        "the module was not given the parent's alias, one level deeper:\n{module}"
    );
    assert_compiles(&workspace);
}

/// The rebased `use` resolves where the seam holds the parent's only use of the type, which is the
/// case the server offers no import for and the pass reconstructs the parent's declaration.
///
/// That is plan 05's teardown seam on `svc_spawn_split_agent.rs`, refused as "left 4 unresolved
/// occurrence(s) of it, where there were 3" although `use super::super::SplitStartFailure;` is the
/// right path from the module the seam becomes.
#[tokio::test(flavor = "multi_thread")]
async fn imports_the_parent_s_type_through_super_where_the_seam_holds_its_only_use() {
    // Given
    let workspace = a_crate_whose_module_imports_a_type_only_the_seam_names_through_super();
    let seam = an_extract_module_of(
        &workspace,
        HOST_MODULE,
        A_SEAM_HOLDING_EVERY_USE_OF_THE_PARENT_S_TYPE,
        "tallying",
    );

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let module = the_module_named(&workspace.read(HOST_MODULE), "tallying");
    assert!(
        module.contains("use super::super::Failure;"),
        "the module was not given the parent's `super::Failure`, one level deeper:\n{module}"
    );
    assert_compiles_with_its_tests(&workspace);
}

/// The same seam moved straight to a file of its own, as plan 05 asks: `to_file`, behind a glob
/// facade.
#[tokio::test(flavor = "multi_thread")]
async fn moves_to_a_file_a_seam_holding_the_only_use_of_the_parent_s_type_through_super() {
    // Given
    let workspace = a_crate_whose_module_imports_a_type_only_the_seam_names_through_super();
    let seam = RefactorOp {
        reexport: Some(Reexport::Glob),
        to_file: true,
        ..an_extract_module_of(
            &workspace,
            HOST_MODULE,
            A_SEAM_HOLDING_EVERY_USE_OF_THE_PARENT_S_TYPE,
            "tallying",
        )
    };

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let module = workspace.read(TALLYING_MODULE);
    assert!(
        module.contains("use super::super::Failure;"),
        "the module was not given the parent's `super::Failure`, one level deeper:\n{module}"
    );
    assert_compiles_with_its_tests(&workspace);
}

/// The import pass runs against names the server can actually judge, even on a server that has
/// only just started.
///
/// This is the same fixture and seam as the D8 guard, handed to the engine as `tddy-tools
/// restructure` hands it over cold. The engine treats a non-null hover as "ready", and it gets one
/// seconds before the server flags any name as unresolved. So the pass finds nothing to import, and
/// the operation reports success over a module that does not compile. This has the same cause as
/// E2's `req: _` (readiness declared before the server has settled), and it is the reason every
/// other test in this file runs settled.
#[tokio::test(flavor = "multi_thread")]
async fn imports_the_alias_the_moved_code_names_on_a_server_that_has_only_just_started() {
    // Given
    let workspace = a_crate_whose_alias_the_server_resolves();
    let seam = an_extract_module_of(&workspace, ORIGIN_LIB, A_SEAM_NAMING_THE_ALIAS, "readings");

    // When
    performing(&workspace, seam).await;

    // Then
    let module = the_module_named(&workspace.read(ORIGIN_LIB), "readings");
    assert_eq!(
        module.matches(THE_PARENT_S_ALIAS).count(),
        1,
        "the module did not import the parent's alias exactly once:\n{module}"
    );
    assert_compiles(&workspace);
}

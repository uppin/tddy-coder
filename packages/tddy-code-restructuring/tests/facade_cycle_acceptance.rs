//! The dependency-cycle refusal, against a live rust-analyzer.
//!
//! The refusal exists for a real hazard: a module whose moved header still names the crate it left,
//! while that crate goes on naming the module, is a cycle the operation would author. But it decides
//! by **extern name alone**, with no notion of where an item is defined — so a back-compat
//! `pub use other_crate::thing;` in the origin makes rust-analyzer canonicalise a caller's
//! `crate::thing` as `origin::thing`, and a clean move is refused.
//!
//! `#unbundle` node 2 hit this on `screen_sharing_service.rs`, which was otherwise exactly the shape
//! the operation supports. Four of the five paths it objected to were the previous node's own
//! facades; the fifth resolved into the crate the module was being moved *to*.
//!
//! Load-sensitive: one server at a time.

mod harness;

use harness::{
    a_move_of, a_workspace_a_module_can_move_across,
    a_workspace_whose_origin_re_exports_what_moves_reaches, assert_compiles, performing,
    refusal_from,
};

const MODULE: &str = "crates/origin/src/host_registry.rs";

/// AC4 — a path the origin merely re-exports is not an origin dependency.
///
/// `config` is `shared`'s. The origin writes `pub use shared::config;` for back-compatibility, which
/// is what every node of a carving stack leaves behind — so a refusal keyed to the re-export would
/// fire on essentially every planned move.
#[tokio::test(flavor = "multi_thread")]
async fn moves_a_module_reaching_through_the_origin_s_own_re_export() {
    // Given
    let workspace = a_workspace_whose_origin_re_exports_what_moves_reaches();

    // When
    performing(
        &workspace,
        a_move_of(
            MODULE,
            "host_registry",
            Some(tddy_code_restructuring::Reexport::Glob),
        ),
    )
    .await;

    // Then
    assert!(
        workspace.holds("crates/destination/src/host_registry.rs"),
        "the module did not arrive — the re-export was read as an origin dependency"
    );
    assert_compiles(&workspace);
}

/// AC4, continued — the destination's manifest names the crate that **defines** the item.
///
/// Resolving the re-export is only half the job: the moved code still needs the dependency, and the
/// one it needs is `shared`'s, not the origin's. A move that resolved the cycle but wrote the wrong
/// manifest line would compile here and fail on the next crate.
#[tokio::test(flavor = "multi_thread")]
async fn gives_the_destination_a_dependency_on_the_defining_crate() {
    // Given
    let workspace = a_workspace_whose_origin_re_exports_what_moves_reaches();

    // When
    performing(
        &workspace,
        a_move_of(
            MODULE,
            "host_registry",
            Some(tddy_code_restructuring::Reexport::Glob),
        ),
    )
    .await;

    // Then
    let manifest = workspace.read("crates/destination/Cargo.toml");
    assert!(
        manifest.contains("shared"),
        "the destination did not gain a dependency on the crate that defines `config`:\n{manifest}"
    );
    assert!(
        !manifest.contains("origin"),
        "the destination gained a dependency on the crate it left:\n{manifest}"
    );
}

/// AC6 — a genuine origin dependency is still refused, with the message it has today.
///
/// This is the case the refusal was written for, and the one that must survive: the moved module
/// names something the origin itself defines, so the destination really would depend on the crate
/// it left.
#[tokio::test(flavor = "multi_thread")]
async fn still_refuses_when_the_origin_defines_what_the_moved_module_names() {
    // Given
    let workspace = a_workspace_a_module_can_move_across();
    workspace.rewriting(
        "crates/origin/src/lib.rs",
        "//! The crate the module leaves.\n\npub mod host_registry;\npub mod runtime;\n\n\
         pub struct OriginOwned;\n",
    );
    workspace.rewriting(
        "crates/origin/src/host_registry.rs",
        "use crate::OriginOwned;\nuse shared::Clock;\n\npub struct HostRegistry {\n    \
         clock: Clock,\n}\n\nimpl HostRegistry {\n    pub fn new() -> Self {\n        \
         Self { clock: Clock }\n    }\n\n    pub fn owned(&self) -> OriginOwned {\n        \
         OriginOwned\n    }\n\n    pub fn stamp(&self) -> u64 {\n        \
         self.clock.now()\n    }\n}\n",
    );

    // When
    let refusal = refusal_from(
        &workspace,
        a_move_of(
            MODULE,
            "host_registry",
            Some(tddy_code_restructuring::Reexport::Glob),
        ),
    )
    .await;

    // Then
    assert!(
        refusal.contains("still names"),
        "the refusal that guards a real cycle no longer fires: {refusal}"
    );
    assert!(
        refusal.contains("origin"),
        "the refusal did not name the crate the destination would depend on: {refusal}"
    );
}

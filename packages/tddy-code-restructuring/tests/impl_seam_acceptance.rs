//! Seams that cut an `impl` in half, against a live rust-analyzer: defect E3.
//!
//! rust-analyzer's `extract_module` over some members of an inherent `impl` writes them as
//! `mod … { use super::Gauge; impl Gauge { … } }`. They stay inherent methods of the same type, so a
//! `self.method()` call resolves from either side of the cut. Today the engine refuses the seam
//! whenever a member left behind calls one that moves, on the premise that the call "would resolve
//! nowhere". That premise is false, and it refuses 5 of the 9 `cli_session_manager.rs` seams.
//!
//! What must still be refused is a cut that can never compile: half of a **trait** `impl`.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use std::ops::RangeInclusive;

use harness::{
    a_crate_whose_impl_member_calls_a_private_one_the_seam_moves,
    a_crate_whose_impl_member_calls_one_the_seam_moves,
    a_crate_whose_moving_impl_member_calls_one_left_behind,
    a_crate_whose_trait_impl_member_calls_a_sibling, an_extract_module_of, assert_compiles,
    performing_once_settled, refusal_once_settled_from, the_module_named, ORIGIN_LIB,
};

/// `doubled`, in each of the inherent-`impl` fixtures.
const THE_SEAM_TAKING_DOUBLED: RangeInclusive<u32> = 12..=14;
/// `doubled`, in the trait-`impl` fixture.
const THE_TRAIT_SEAM_TAKING_DOUBLED: RangeInclusive<u32> = 17..=19;

/// `reading` stays behind and calls `self.doubled()`, which moves. The call is reached through
/// `Gauge`, and `doubled` is still a method of `Gauge`.
#[tokio::test(flavor = "multi_thread")]
async fn moves_a_method_that_a_member_left_behind_calls() {
    // Given
    let workspace = a_crate_whose_impl_member_calls_one_the_seam_moves();
    let seam = an_extract_module_of(&workspace, ORIGIN_LIB, THE_SEAM_TAKING_DOUBLED, "doubling");

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let module = the_module_named(&workspace.read(ORIGIN_LIB), "doubling");
    assert!(
        module.contains("fn doubled(&self) -> u32 {"),
        "`doubled` did not move into the new module:\n{module}"
    );
    assert_compiles(&workspace);
}

/// The same, where the method that moves is private. A private method in the new module is private
/// to that module, so the call left behind stays callable only if the method is made visible to it.
#[tokio::test(flavor = "multi_thread")]
async fn moves_a_private_method_that_a_member_left_behind_calls() {
    // Given
    let workspace = a_crate_whose_impl_member_calls_a_private_one_the_seam_moves();
    let seam = an_extract_module_of(&workspace, ORIGIN_LIB, THE_SEAM_TAKING_DOUBLED, "doubling");

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let module = the_module_named(&workspace.read(ORIGIN_LIB), "doubling");
    assert!(
        module.contains("fn doubled(&self) -> u32 {"),
        "`doubled` did not move into the new module:\n{module}"
    );
    assert_compiles(&workspace);
}

/// The other direction: `doubled` moves and calls the private `base`, which stays. A child module
/// sees its parent's private items, so nothing needs widening. Whatever fixes E3 must keep this
/// applying.
#[tokio::test(flavor = "multi_thread")]
async fn moves_a_method_that_calls_one_left_behind() {
    // Given
    let workspace = a_crate_whose_moving_impl_member_calls_one_left_behind();
    let seam = an_extract_module_of(&workspace, ORIGIN_LIB, THE_SEAM_TAKING_DOUBLED, "doubling");

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let module = the_module_named(&workspace.read(ORIGIN_LIB), "doubling");
    assert!(
        module.contains("self.base() * 2"),
        "`doubled` did not move into the new module:\n{module}"
    );
    assert_compiles(&workspace);
}

/// Half of a trait `impl` cannot move. The new module would hold a second `impl Meter for Gauge`,
/// and each half would lack the other's items. Relaxing E3 for inherent methods must not relax
/// this.
#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_seam_that_cuts_a_trait_impl_in_half() {
    // Given
    let workspace = a_crate_whose_trait_impl_member_calls_a_sibling();
    let seam = an_extract_module_of(
        &workspace,
        ORIGIN_LIB,
        THE_TRAIT_SEAM_TAKING_DOUBLED,
        "doubling",
    );

    // When
    let refusal = refusal_once_settled_from(&workspace, seam).await;

    // Then
    assert!(
        refusal.starts_with("this seam cannot be cut here"),
        "half of a trait `impl` was not refused as a seam: {refusal}"
    );
    assert!(
        refusal.contains("`doubled`"),
        "the refusal did not name the member it would have split off: {refusal}"
    );
}

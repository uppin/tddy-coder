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
    a_crate_whose_impl_member_calls_an_associated_fn_the_seam_moves,
    a_crate_whose_impl_member_calls_one_the_seam_moves,
    a_crate_whose_moving_impl_member_calls_one_left_behind,
    a_crate_whose_tests_call_an_associated_fn_the_seam_moves_through_an_alias,
    a_crate_whose_tests_call_an_associated_fn_the_seam_moves_through_the_type,
    a_crate_whose_trait_impl_member_calls_a_sibling, an_extract_module_of, assert_compiles,
    assert_compiles_with_its_tests, performing_once_settled, refusal_once_settled_from,
    the_module_named, ORIGIN_LIB,
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

/// `reading` stays behind and calls `Self::doubled(…)`, an associated function that moves. The
/// path is reached through the type, and `doubled` is still an associated function of `Gauge`.
/// That is plan 02's `Self::build_claude_argv(…)` in `cli_session_manager.rs`.
#[tokio::test(flavor = "multi_thread")]
async fn moves_an_associated_function_a_member_left_behind_calls_through_self() {
    // Given
    let workspace = a_crate_whose_impl_member_calls_an_associated_fn_the_seam_moves();
    let seam = an_extract_module_of(&workspace, ORIGIN_LIB, THE_SEAM_TAKING_DOUBLED, "doubling");

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("        Self::doubled(self.level) + 1"),
        "the call left behind is no longer `Self::doubled`:\n{lib}"
    );
    assert_compiles(&workspace);
}

/// The file's tests call the moved associated function through the type's name,
/// `Gauge::doubled(2)`, which resolves wherever its `impl` lives.
#[tokio::test(flavor = "multi_thread")]
async fn moves_an_associated_function_the_file_s_tests_call_through_the_type() {
    // Given
    let workspace = a_crate_whose_tests_call_an_associated_fn_the_seam_moves_through_the_type();
    let seam = an_extract_module_of(&workspace, ORIGIN_LIB, THE_SEAM_TAKING_DOUBLED, "doubling");

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("        let doubled = Gauge::doubled(2);"),
        "the test's call is no longer `Gauge::doubled`:\n{lib}"
    );
    assert_compiles_with_its_tests(&workspace);
}

/// The same through a type alias, `Meter::doubled(2)`: `ClaudeCliSessionManager::build_claude_argv`.
#[tokio::test(flavor = "multi_thread")]
async fn moves_an_associated_function_the_file_s_tests_call_through_a_type_alias() {
    // Given
    let workspace = a_crate_whose_tests_call_an_associated_fn_the_seam_moves_through_an_alias();
    let seam = an_extract_module_of(&workspace, ORIGIN_LIB, THE_SEAM_TAKING_DOUBLED, "doubling");

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("        let doubled = Meter::doubled(2);"),
        "the test's call is no longer `Meter::doubled`:\n{lib}"
    );
    assert_compiles_with_its_tests(&workspace);
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

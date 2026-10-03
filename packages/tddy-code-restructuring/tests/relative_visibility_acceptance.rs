//! A relative visibility across a seam, against a live rust-analyzer.
//!
//! `pub(super)` names the parent of the module it is written in. An item moved into a new child of
//! that module and put back "as written" would read `pub(super)` inside the child, which means the
//! module it came from: its parent's `pub(super) use` facade is `E0364` and every caller outside is
//! `E0603`. The item has to be written `pub(in super::super)` to stay visible where it was.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use harness::{
    a_crate_whose_module_declares_a_function_visible_in_its_parent, an_extract_module_of,
    assert_compiles, performing_once_settled, OUTER_MODULE,
};
use tddy_code_restructuring::Reexport;

/// The whole of `outer.rs` below its doc comment: `helper`, `shared` and `combined`.
const THE_THREE_FUNCTIONS: std::ops::RangeInclusive<u32> = 3..=13;

const THE_MOVED_FILE: &str = "crates/origin/src/outer/inner.rs";

fn splitting_into_a_file_with_a_named_facade(
    workspace: &harness::AFixtureWorkspace,
) -> tddy_code_restructuring::RefactorOp {
    let mut seam = an_extract_module_of(workspace, OUTER_MODULE, THE_THREE_FUNCTIONS, "inner");
    seam.reexport = Some(Reexport::Named);
    seam.to_file = true;
    seam
}

/// `helper` and `combined` are `pub(super)` in `outer`, and the crate root calls them through the
/// facade. Written `pub(super)` in `outer::inner` they are visible in `outer` only.
#[tokio::test(flavor = "multi_thread")]
async fn keeps_a_pub_super_function_visible_in_the_module_it_was_visible_in() {
    // Given
    let workspace = a_crate_whose_module_declares_a_function_visible_in_its_parent();
    let seam = splitting_into_a_file_with_a_named_facade(&workspace);

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    assert_compiles(&workspace);
    let moved = workspace.read(THE_MOVED_FILE);
    assert!(
        moved.contains("pub(in super::super) fn helper() -> u32 {"),
        "the moved function does not read as visible in `outer`'s parent:\n{moved}"
    );
}

/// An absolute visibility means the same one level down, so it is written as it was.
#[tokio::test(flavor = "multi_thread")]
async fn leaves_a_pub_crate_function_as_it_was_written() {
    // Given
    let workspace = a_crate_whose_module_declares_a_function_visible_in_its_parent();
    let seam = splitting_into_a_file_with_a_named_facade(&workspace);

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let moved = workspace.read(THE_MOVED_FILE);
    assert!(
        moved.contains("pub(crate) fn shared() -> u32 {"),
        "the `pub(crate)` function was rewritten:\n{moved}"
    );
}

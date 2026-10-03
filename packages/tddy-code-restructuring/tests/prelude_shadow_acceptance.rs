//! A name the parent rebinds over the prelude, against a live rust-analyzer.
//!
//! `use crate::Result;` is the crate's one-generic alias. Moved into a child module, the
//! name still resolves — to the prelude's two-generic `std::result::Result` — so the server reports
//! nothing unresolved and the server-driven import pass restores nothing. The child then means a
//! different type, and the compiler is the first to say so (E0107). Each seam is judged by what
//! `cargo check` says of the tree afterwards.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use std::ops::RangeInclusive;

use harness::{
    a_crate_whose_parent_rebinds_a_prelude_name, an_extract_module_of, assert_compiles,
    performing_once_settled, the_module_named, ENTRY_MODULE,
};

/// `run` and `pick` of `entry.rs`: the first names the parent's `Result`, the second the prelude's `Option`.
const A_SEAM_NAMING_THE_REBOUND_RESULT: RangeInclusive<u32> = 13..=23;

#[tokio::test(flavor = "multi_thread")]
async fn gives_the_module_the_parent_s_import_of_a_name_that_shadows_the_prelude() {
    // Given
    let workspace = a_crate_whose_parent_rebinds_a_prelude_name();
    let seam = an_extract_module_of(
        &workspace,
        ENTRY_MODULE,
        A_SEAM_NAMING_THE_REBOUND_RESULT,
        "runs",
    );

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let module = the_module_named(&workspace.read(ENTRY_MODULE), "runs");
    assert!(
        module.contains("use crate::Result;"),
        "the module did not get the parent's `Result`:\n{module}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn gives_the_module_no_import_for_a_prelude_name_the_parent_never_imported() {
    // Given
    let workspace = a_crate_whose_parent_rebinds_a_prelude_name();
    let seam = an_extract_module_of(
        &workspace,
        ENTRY_MODULE,
        A_SEAM_NAMING_THE_REBOUND_RESULT,
        "runs",
    );

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let module = the_module_named(&workspace.read(ENTRY_MODULE), "runs");
    assert!(
        !module.contains("Option;"),
        "the module was given an import for `Option`:\n{module}"
    );
}

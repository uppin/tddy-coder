//! `reparent_module` where the moved module carries more than a function: relative paths from its
//! children, an inline test module, a `mod.rs` shape, a private declaration its caller needs
//! widened, and the crate root as the new parent.
//!
//! Each of these compiles before the move and has to compile — with its tests, and under clippy —
//! after it, which is what an edit that merely looks right cannot satisfy.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use harness::{assert_compiles_with_its_tests, assert_lints_clean};
use same_crate::{an_app_holding, reparenting_module};

const LIB: &str = "pub mod audit;\npub mod host;\npub mod split;\n";
const THE_HOST_ITSELF: &str = "pub fn host_name() -> &'static str {\n    \"host\"\n}\n";
const AN_IDLE_MODULE: &str = "pub fn idle() -> u32 {\n    0\n}\n";

/// AC1 — a child reaches two levels up, and an inline test module reaches its own module.
#[tokio::test(flavor = "multi_thread")]
async fn keeps_a_childs_relative_path_and_an_inline_test_module_meaning_what_they_meant() {
    // Given `attachments` with a child that reaches `host` two levels up and a test module
    let workspace = an_app_holding(&[
        ("src/lib.rs", LIB),
        (
            "src/host.rs",
            &format!("pub mod attachments;\n\n{THE_HOST_ITSELF}"),
        ),
        (
            "src/host/attachments.rs",
            concat!(
                "pub mod staging;\n",
                "\n",
                "// Stages one attachment.\n",
                "pub fn materialize() -> u32 {\n",
                "    staging::stage()\n",
                "}\n",
                "\n",
                "#[cfg(test)]\n",
                "mod tests {\n",
                "    use super::*;\n",
                "\n",
                "    #[test]\n",
                "    fn stages() {\n",
                "        assert_eq!(materialize(), 4);\n",
                "    }\n",
                "}\n",
            ),
        ),
        (
            "src/host/attachments/staging.rs",
            concat!(
                "use super::super::host_name;\n",
                "\n",
                "pub fn stage() -> u32 {\n",
                "    host_name().len() as u32\n",
                "}\n",
            ),
        ),
        ("src/split.rs", AN_IDLE_MODULE),
        ("src/audit.rs", AN_IDLE_MODULE),
    ]);

    // When it is re-parented under `split`
    reparenting_module(&workspace, "src/host.rs", "attachments", "app::split", None)
        .await
        .expect("the re-parent applies");

    // Then everything still compiles, its tests included, under clippy, and the comment survived
    assert_compiles_with_its_tests(&workspace);
    assert_lints_clean(&workspace);
    assert!(
        workspace
            .read("src/split/attachments.rs")
            .contains("// Stages one attachment.\npub fn materialize"),
        "the module's own text was not carried over as written:\n{}",
        workspace.read("src/split/attachments.rs")
    );
}

/// AC2 — a module that is a `mod.rs` arrives as one, with its children beside it.
#[tokio::test(flavor = "multi_thread")]
async fn moves_a_module_that_is_a_mod_rs_as_a_mod_rs() {
    // Given `attachments` as `src/host/attachments/mod.rs` with a child
    let workspace = an_app_holding(&[
        ("src/lib.rs", LIB),
        (
            "src/host.rs",
            &format!("pub mod attachments;\n\n{THE_HOST_ITSELF}"),
        ),
        (
            "src/host/attachments/mod.rs",
            "mod staging;\n\npub fn materialize() -> u32 {\n    staging::stage()\n}\n",
        ),
        (
            "src/host/attachments/staging.rs",
            "pub fn stage() -> u32 {\n    2\n}\n",
        ),
        ("src/split.rs", AN_IDLE_MODULE),
        ("src/audit.rs", AN_IDLE_MODULE),
    ]);

    // When it is re-parented under `split`
    reparenting_module(&workspace, "src/host.rs", "attachments", "app::split", None)
        .await
        .expect("the re-parent applies");

    // Then both files arrived under `split`, in the shape they had
    assert!(workspace.holds("src/split/attachments/mod.rs"));
    assert!(workspace.holds("src/split/attachments/staging.rs"));
    assert!(!workspace.holds("src/host/attachments/mod.rs"));
    assert_compiles_with_its_tests(&workspace);
}

/// AC3 — a private declaration is widened to what its caller needs, and the caller is re-pointed.
#[tokio::test(flavor = "multi_thread")]
async fn widens_a_private_declaration_only_as_far_as_its_caller_needs() {
    // Given a private `attachments` that only its old parent reaches, through a bare name
    let workspace = an_app_holding(&[
        ("src/lib.rs", LIB),
        (
            "src/host.rs",
            "mod attachments;\n\npub fn host_name() -> u32 {\n    attachments::materialize()\n}\n",
        ),
        (
            "src/host/attachments.rs",
            "pub fn materialize() -> u32 {\n    1\n}\n",
        ),
        ("src/split.rs", AN_IDLE_MODULE),
        ("src/audit.rs", AN_IDLE_MODULE),
    ]);

    // When it is re-parented under `split`
    reparenting_module(&workspace, "src/host.rs", "attachments", "app::split", None)
        .await
        .expect("the re-parent applies");

    // Then `split` declares it visible to the crate, no wider, and `host` imports it from there
    assert!(
        workspace
            .read("src/split.rs")
            .contains("pub(crate) mod attachments;"),
        "the declaration was not widened to the crate:\n{}",
        workspace.read("src/split.rs")
    );
    assert!(
        workspace
            .read("src/host.rs")
            .contains("use crate::split::attachments;"),
        "the old parent still names a module it no longer declares:\n{}",
        workspace.read("src/host.rs")
    );
    assert_compiles_with_its_tests(&workspace);
    assert_lints_clean(&workspace);
}

/// AC4 — the crate root is a parent like any other, and its file sits beside `lib.rs`.
#[tokio::test(flavor = "multi_thread")]
async fn moves_a_module_up_to_the_crate_root() {
    // Given `attachments` under `host`, and a caller that names it inline
    let workspace = an_app_holding(&[
        ("src/lib.rs", LIB),
        (
            "src/host.rs",
            &format!("pub mod attachments;\n\n{THE_HOST_ITSELF}"),
        ),
        (
            "src/host/attachments.rs",
            "pub fn materialize() -> u32 {\n    1\n}\n",
        ),
        ("src/split.rs", AN_IDLE_MODULE),
        (
            "src/audit.rs",
            "pub fn audit() -> u32 {\n    crate::host::attachments::materialize()\n}\n",
        ),
    ]);

    // When it is re-parented under the crate root
    reparenting_module(&workspace, "src/host.rs", "attachments", "app", None)
        .await
        .expect("the re-parent applies");

    // Then the file is beside `lib.rs`, `lib.rs` declares it, and the caller follows
    assert!(workspace.holds("src/attachments.rs"));
    assert!(workspace
        .read("src/lib.rs")
        .contains("pub mod attachments;"));
    assert!(workspace
        .read("src/audit.rs")
        .contains("crate::attachments::materialize()"));
    assert_compiles_with_its_tests(&workspace);
}

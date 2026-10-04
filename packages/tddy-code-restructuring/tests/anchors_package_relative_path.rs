//! `restructure anchors` told an author "is in no package" when they wrote a path relative to the
//! package they were standing in, and did not say what to write instead.
//!
//! Every path a plan carries is relative to the **repo root**: the run's one workspace root is the
//! directory it stands in. An author who has `cd`ed into a package and wrote `src/lib.rs` is giving
//! the natural path of the file they are looking at, and the refusal they met named the problem
//! (no package above it) without naming the fix.
//!
//! The path is deliberately **not** resolved for them. A guess that succeeds is a fallback, and a
//! plan written from one would carry a path that means something else from the next directory.

mod harness;

use harness::{a_workspace_holding_files, the_anchor_command_emits};

const A_WORKSPACE_OF_ONE_PACKAGE_BELOW_THE_ROOT: [(&str, &str); 3] = [
    (
        "Cargo.toml",
        "[workspace]\nresolver = \"2\"\nmembers = [\"crates/app\"]\n",
    ),
    (
        "crates/app/Cargo.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    ),
    ("crates/app/src/lib.rs", "pub fn answer() -> u32 {\n    42\n}\n"),
];

#[tokio::test(flavor = "multi_thread")]
async fn names_the_repo_root_path_when_given_one_relative_to_a_package() {
    // Given a package that lives below the repo root
    let workspace = a_workspace_holding_files(&A_WORKSPACE_OF_ONE_PACKAGE_BELOW_THE_ROOT);

    // When `anchors` is asked about a path relative to that package
    let refusal = the_anchor_command_emits(&workspace, "src/lib.rs", &["answer"], None)
        .await
        .expect_err("a path relative to a package is not a path from the repo root");

    // Then the refusal names the path to write instead
    assert!(
        refusal.contains("crates/app/src/lib.rs"),
        "the refusal did not name the repo-root path to write: {refusal}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn still_refuses_it_rather_than_resolving_it_silently() {
    // Given the same package
    let workspace = a_workspace_holding_files(&A_WORKSPACE_OF_ONE_PACKAGE_BELOW_THE_ROOT);

    // When `anchors` is asked about the package-relative path
    let outcome = the_anchor_command_emits(&workspace, "src/lib.rs", &["answer"], None).await;

    // Then it is refused: a plan carries repo-root paths, and a guess that works here would mean
    // something else from another directory
    assert!(
        outcome.is_err(),
        "a package-relative path was accepted as if it were a repo-root one: {outcome:?}"
    );
}

//! `repoint_facade_imports` through the runner, against a live rust-analyzer and a real compiler.
//!
//! The library-level suite (`repoint_facade_imports_acceptance.rs`) pins the edits and the refusals
//! byte for byte and needs no server. What only a live run can say is that the rewritten tree still
//! compiles, that rustfmt (which `apply` runs over every file it wrote) leaves the result clean, and
//! that the list `check --deep` prints is the list `apply` then acts on.
//!
//! `cargo check --all-targets` is the oracle no edit that merely looks right can satisfy.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod facade_imports;
mod harness;
mod same_crate;

use std::sync::{Arc, Mutex};

use facade_imports::{a_repoint_of_the_file, the_notes_of};
use harness::{
    a_workspace_holding_files, applying_the_plan_with, assert_compiles_with_its_tests,
    checking_the_plan_with, AFixtureWorkspace,
};
use same_crate::the_anchor_over;
use tddy_code_restructuring::{Anchor, RefactorOp};

const THE_KERNEL: [(&str, &str); 3] = [
    (
        "kernel/Cargo.toml",
        "[package]\nname = \"kernel\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    ),
    ("kernel/src/lib.rs", "pub mod config;\n"),
    (
        "kernel/src/config.rs",
        "pub struct Settings {\n    pub verbose: bool,\n}\n\npub struct Limits {\n    pub max: u32,\n}\n\npub fn standard_limits() -> Limits {\n    Limits { max: 8 }\n}\n",
    ),
];

const THE_MIDDLE: [(&str, &str); 2] = [
    (
        "mid/Cargo.toml",
        "[package]\nname = \"mid\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nkernel = { path = \"../kernel\" }\n",
    ),
    ("mid/src/lib.rs", "pub use kernel::config;\n"),
];

const THE_APP_MANIFEST: (&str, &str) = (
    "app/Cargo.toml",
    "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nkernel = { path = \"../kernel\" }\nmid = { path = \"../mid\" }\n",
);

const THE_WORKSPACE: (&str, &str) = (
    "Cargo.toml",
    "[workspace]\nresolver = \"2\"\nmembers = [\"app\", \"kernel\", \"mid\"]\n",
);

/// What `app/src/a.rs` imports and calls through the facades of `lib.rs`.
const A_FILE_OF_FACADE_IMPORTS: &str = concat!(
    "use crate::cfg::Settings;\n",
    "use crate::{b::Thing, config::Limits};\n",
    "\n",
    "pub fn describe(_: &Thing) -> u32 {\n",
    "    let settings = Settings { verbose: false };\n",
    "    let limits: Limits = crate::config::standard_limits();\n",
    "    limits.max + settings.verbose as u32\n",
    "}\n",
    "\n",
    "#[cfg(test)]\n",
    "mod tests {\n",
    "    use crate::config::standard_limits;\n",
    "\n",
    "    #[test]\n",
    "    fn the_standard_limit_is_eight() {\n",
    "        assert_eq!(standard_limits().max, 8);\n",
    "    }\n",
    "}\n",
);

/// Three crates: `kernel` defines, `mid` re-exports, and `app` re-exports both ways in `lib.rs`.
fn an_app_over_a_facade_chain(app_files: &[(&str, &str)]) -> AFixtureWorkspace {
    let mut files: Vec<(&str, &str)> = vec![THE_WORKSPACE, THE_APP_MANIFEST];
    files.extend(THE_KERNEL);
    files.extend(THE_MIDDLE);
    files.extend_from_slice(app_files);
    a_workspace_holding_files(&files)
}

fn the_app_of_one_file() -> AFixtureWorkspace {
    an_app_over_a_facade_chain(&[
        (
            "app/src/lib.rs",
            "pub use kernel::config;\npub use mid::config as cfg;\npub mod a;\npub mod b;\n",
        ),
        ("app/src/a.rs", A_FILE_OF_FACADE_IMPORTS),
        ("app/src/b.rs", "pub struct Thing;\n"),
    ])
}

/// The notes an operation's account holds, kept as the run goes.
fn an_account_that_keeps_what_it_hears() -> (
    tddy_code_restructuring::backends::rust::ProgressSink,
    Arc<Mutex<Vec<String>>>,
) {
    harness::a_sink_that_keeps_what_it_hears()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_of_facade_imports_is_re_pointed_and_the_workspace_still_compiles_with_its_tests() {
    // Given a file importing and calling through two facades
    let workspace = the_app_of_one_file();
    let plan = workspace.a_plan_of(&[a_repoint_of_the_file("app/src/a.rs")]);

    // When the plan is applied through the runner
    let outcome = applying_the_plan_with(&workspace, plan, |_| {}).await;

    // Then it succeeds, no path goes through a facade any more, the tree compiles with its tests, and the file is rustfmt-clean
    outcome.expect("the apply succeeds");
    let after = workspace.read("app/src/a.rs");
    assert!(
        !after.contains("crate::cfg") && !after.contains("crate::config"),
        "a path still goes through a facade:\n{after}"
    );
    assert!(after.contains("kernel::config::Limits"), "{after}");
    assert_compiles_with_its_tests(&workspace);
    assert!(workspace.is_rustfmt_clean("app/src/a.rs"), "{after}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deep_check_lists_what_an_apply_then_rewrites_and_writes_nothing_itself() {
    // Given the same file, and the plan that re-points it
    let workspace = the_app_of_one_file();
    let plan = workspace.a_plan_of(&[a_repoint_of_the_file("app/src/a.rs")]);
    let (checking, checked) = an_account_that_keeps_what_it_hears();
    let (applying, applied) = an_account_that_keeps_what_it_hears();

    // When the plan is deep-checked, and then applied
    let findings = checking_the_plan_with(&workspace, plan.clone(), true, |options| {
        options.account = checking;
    })
    .await
    .expect("the deep check runs");
    let untouched = workspace.read("app/src/a.rs");
    assert_eq!(findings, Vec::<String>::new(), "the deep check found fault");
    applying_the_plan_with(&workspace, plan, |options| options.account = applying)
        .await
        .expect("the apply succeeds");

    // Then the check wrote nothing, and the notes of the two accounts are the same lines
    assert_eq!(untouched, A_FILE_OF_FACADE_IMPORTS);
    let (checked, applied) = (
        the_notes_of(&checked.lock().expect("the account")),
        the_notes_of(&applied.lock().expect("the account")),
    );
    assert!(checked.len() > 1, "the check listed no path: {checked:?}");
    assert_eq!(checked, applied);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_module_anchored_run_re_points_every_file_of_the_module_and_leaves_siblings_alone() {
    // Given a module of two files and a sibling, each importing through the facade
    let through_the_facade = "use crate::config::Settings;\n\npub fn s(_: &Settings) {}\n";
    let workspace = an_app_over_a_facade_chain(&[
        (
            "app/src/lib.rs",
            "pub use kernel::config;\npub mod a;\npub mod b;\n",
        ),
        (
            "app/src/a.rs",
            &format!("{through_the_facade}\npub mod nested;\n"),
        ),
        ("app/src/a/nested.rs", through_the_facade),
        ("app/src/b.rs", through_the_facade),
    ]);
    let Anchor::Items {
        file,
        items,
        fingerprints,
    } = the_anchor_over(&workspace, "app/src/lib.rs", &["a"]).await
    else {
        panic!("the anchors command emits an `items` anchor over a name");
    };
    let module: RefactorOp = serde_json::from_value(serde_json::json!({
        "op": "repoint_facade_imports",
        "anchor": { "kind": "items", "file": file, "items": items, "fingerprints": fingerprints },
    }))
    .expect("the operation reads");
    let plan = workspace.a_plan_of(&[module]);

    // When the plan is applied
    applying_the_plan_with(&workspace, plan, |_| {})
        .await
        .expect("the apply succeeds");

    // Then both files of the module are re-pointed, the sibling is as it was, and the tree compiles
    assert!(workspace
        .read("app/src/a.rs")
        .contains("use kernel::config::Settings;"));
    assert!(workspace
        .read("app/src/a/nested.rs")
        .contains("use kernel::config::Settings;"));
    assert_eq!(workspace.read("app/src/b.rs"), through_the_facade);
    assert_compiles_with_its_tests(&workspace);
}

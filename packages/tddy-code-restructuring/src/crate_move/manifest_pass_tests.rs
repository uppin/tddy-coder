//! The destination's manifest gains every crate the moved code names: one imported under its own
//! name (`use async_trait::async_trait;`), and one its origin declares only for a target
//! (`[target.'cfg(unix)'.dependencies]`), which lands in the same target table.
//!
//! `#carve` 21/21 fixed both by hand after the compile gate failed — `async-trait` in
//! `tddy-demo-vm-service`, `libc` in `tddy-cli-sessions` (flattened into `[dependencies]`).

use super::{resolve, ItemReferences, ModuleReferences};
use crate::edit::WorkspaceEdit;
use crate::overlay::Overlay;
use crate::plan::{Plan, RefactorOp};
use crate::registry::Workspace;
use crate::Result;

const HEADER: &str = r#"{"v":1,"snapshot":{}}"#;
const MODULE: &str = "crates/origin/src/host_registry.rs";
const DESTINATION_MANIFEST: &str = "crates/destination/Cargo.toml";
const PACKAGE_DESTINATION: &str =
    "[package]\nname = \"destination\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const PACKAGE_ORIGIN: &str =
    "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

/// Nothing outside the moving file reaches it, so the manifest pass is the only thing under test.
struct NoOutsideReferences;

impl ModuleReferences for NoOutsideReferences {
    fn outside_references(
        &mut self,
        _workspace: &Workspace<'_>,
        _file: &str,
    ) -> Result<Vec<ItemReferences>> {
        Ok(Vec::new())
    }
}

/// `crates/origin` declaring `host_registry`, and an existing `crates/destination`.
struct AWorkspace {
    directory: tempfile::TempDir,
    overlay: Overlay,
}

/// An origin whose manifest is `[package]` followed by `dependency_tables`, moving `host_registry`
/// written as `module`.
fn an_origin_declaring(dependency_tables: &str, module: &str) -> AWorkspace {
    AWorkspace {
        directory: tempfile::tempdir().expect("a temporary directory"),
        overlay: Overlay::new(),
    }
    .with(
        "Cargo.toml",
        "[workspace]\nmembers = [\n    \"crates/origin\",\n    \"crates/destination\",\n]\n",
    )
    .with(
        "crates/origin/Cargo.toml",
        &format!("{PACKAGE_ORIGIN}\n{dependency_tables}"),
    )
    .with("crates/origin/src/lib.rs", "pub mod host_registry;\n")
    .with(MODULE, module)
    .with(DESTINATION_MANIFEST, PACKAGE_DESTINATION)
    .with("crates/destination/src/lib.rs", "")
}

impl AWorkspace {
    fn with(self, relative: &str, text: &str) -> Self {
        let absolute = self.directory.path().join(relative);
        std::fs::create_dir_all(absolute.parent().expect("a parent")).expect("directories");
        std::fs::write(absolute, text).expect("the file is written");
        self
    }

    fn workspace(&self) -> Workspace<'_> {
        Workspace {
            root: self.directory.path(),
            overlay: &self.overlay,
        }
    }

    fn moving_the_host_registry(&self) -> Result<WorkspaceEdit> {
        resolve(
            &mut NoOutsideReferences,
            &self.workspace(),
            &a_move_of_the_host_registry(),
        )
    }

    /// The destination's manifest once the move of `host_registry` is applied.
    fn destination_manifest_after_the_move(&self) -> String {
        let edit = self.moving_the_host_registry().expect("the move resolves");
        let mut overlay = self.overlay.clone();
        overlay
            .record(self.directory.path(), &edit)
            .expect("the edit folds into the overlay");
        overlay
            .read(
                self.directory.path(),
                std::path::Path::new(DESTINATION_MANIFEST),
            )
            .expect("the manifest reads")
    }
}

fn a_move_of_the_host_registry() -> RefactorOp {
    Plan::parse(&format!(
        "{HEADER}\n{}\n",
        r#"{"op":"move_module_to_crate","anchor":{"kind":"symbol","file":"crates/origin/src/host_registry.rs","path":"host_registry"},"to":"crates/destination","reexport":"none"}"#
    ))
    .expect("the plan parses")
    .ops
    .remove(0)
}

/// Test 19 — `use async_trait::async_trait;` binds `async_trait`, and that is not the file's own
/// name for something: it is the crate, and the destination needs it.
#[test]
fn a_moved_file_importing_async_trait_adds_async_trait_to_the_destination() {
    // Given a moved file importing the attribute under its crate's own name
    let workspace = an_origin_declaring(
        "[dependencies]\nasync-trait = \"0.1\"\n",
        "use async_trait::async_trait;\n\n#[async_trait]\npub trait Registry {\n    \
         async fn stamp(&self) -> u64;\n}\n",
    );

    // When it moves
    let manifest = workspace.destination_manifest_after_the_move();

    // Then the destination declares the crate, its line copied verbatim
    assert_eq!(
        manifest,
        format!("{PACKAGE_DESTINATION}\n[dependencies]\nasync-trait = \"0.1\"\n")
    );
}

/// Test 20 — the same shape for a macro and a body path: `use anyhow::anyhow;` with
/// `anyhow::Result`.
#[test]
fn a_moved_file_importing_anyhow_by_its_own_name_adds_anyhow() {
    // Given a moved file importing `anyhow!` under its crate's name and naming `anyhow::Result`
    let workspace = an_origin_declaring(
        "[dependencies]\nanyhow = \"1\"\n",
        "use anyhow::anyhow;\n\npub fn stamp() -> anyhow::Result<u64> {\n    \
         Err(anyhow!(\"no stamp\"))\n}\n",
    );

    // When it moves
    let manifest = workspace.destination_manifest_after_the_move();

    // Then the destination declares `anyhow`
    assert_eq!(
        manifest,
        format!("{PACKAGE_DESTINATION}\n[dependencies]\nanyhow = \"1\"\n")
    );
}

/// Test 21 — a crate the origin declares only for unix is carried into the destination's unix
/// table, not flattened into `[dependencies]`.
#[test]
fn a_crate_the_origin_declares_only_for_unix_lands_in_the_destinations_unix_table() {
    // Given a body path into `libc`, which the origin declares only under `cfg(unix)`
    let workspace = an_origin_declaring(
        "[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n",
        "pub fn alive(pid: i32) -> bool {\n    unsafe { libc::kill(pid, 0) == 0 }\n}\n",
    );

    // When it moves
    let manifest = workspace.destination_manifest_after_the_move();

    // Then the destination gains the same target table with the same line
    assert_eq!(
        manifest,
        format!("{PACKAGE_DESTINATION}\n[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n")
    );
}

/// Test 22 — a `use` of a target-only crate was refused as declared nowhere; it is carried.
#[test]
fn a_use_of_a_crate_declared_only_for_a_target_is_carried_rather_than_refused() {
    // Given a `use` of `libc`, declared only under `cfg(unix)`
    let workspace = an_origin_declaring(
        "[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n",
        "use libc::pid_t;\n\npub fn own() -> pid_t {\n    0\n}\n",
    );

    // When it moves
    let manifest = workspace.destination_manifest_after_the_move();

    // Then the line is carried into the same table
    assert_eq!(
        manifest,
        format!("{PACKAGE_DESTINATION}\n[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n")
    );
}

/// Test 23 — a crate the origin declares for every target is a plain dependency of the destination;
/// its target-specific line adds nothing beside it.
#[test]
fn a_crate_declared_both_plainly_and_for_a_target_goes_to_the_plain_table_only() {
    // Given `log` declared plainly and again under `cfg(unix)`
    let workspace = an_origin_declaring(
        "[dependencies]\nlog = \"0.4\"\n\n[target.'cfg(unix)'.dependencies]\n\
         log = { version = \"0.4\", features = [\"std\"] }\n",
        "pub fn stamp() {\n    log::info!(\"stamp\");\n}\n",
    );

    // When it moves
    let manifest = workspace.destination_manifest_after_the_move();

    // Then only the plain line is carried
    assert_eq!(
        manifest,
        format!("{PACKAGE_DESTINATION}\n[dependencies]\nlog = \"0.4\"\n")
    );
}

/// Test 24 — a crate only `#[cfg(test)]` code names, declared for a target's dev builds, lands in
/// the same target's `dev-dependencies`.
#[test]
fn a_crate_only_test_code_names_lands_in_the_target_dev_dependencies_table() {
    // Given test code naming `nix`, which the origin declares only for unix dev builds
    let workspace = an_origin_declaring(
        "[target.'cfg(unix)'.dev-dependencies]\nnix = \"0.29\"\n",
        "pub fn stamp() -> u32 {\n    1\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    \
         fn runs_in_a_process() {\n        assert!(nix::unistd::getpid().as_raw() > 0);\n    }\n}\n",
    );

    // When it moves
    let manifest = workspace.destination_manifest_after_the_move();

    // Then the destination gains the target's dev table
    assert_eq!(
        manifest,
        format!("{PACKAGE_DESTINATION}\n[target.'cfg(unix)'.dev-dependencies]\nnix = \"0.29\"\n")
    );
}

/// Test 25 — a target table the destination already has is extended, never written twice.
#[test]
fn a_target_table_the_destination_already_has_gains_the_line_under_it_not_a_second_header() {
    // Given a destination with its own unix table
    let workspace = an_origin_declaring(
        "[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n",
        "pub fn alive(pid: i32) -> bool {\n    unsafe { libc::kill(pid, 0) == 0 }\n}\n",
    )
    .with(
        DESTINATION_MANIFEST,
        &format!("{PACKAGE_DESTINATION}\n[target.'cfg(unix)'.dependencies]\nbytes = \"1\"\n"),
    );

    // When it moves
    let manifest = workspace.destination_manifest_after_the_move();

    // Then the line joins the existing table
    assert_eq!(
        manifest,
        format!(
            "{PACKAGE_DESTINATION}\n[target.'cfg(unix)'.dependencies]\nbytes = \"1\"\nlibc = \"0.2\"\n"
        )
    );
}

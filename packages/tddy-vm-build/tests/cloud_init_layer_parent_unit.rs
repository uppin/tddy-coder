//! Unit tests for how the `cloud-init` subcommand picks the image its new layer chains onto.
//!
//! No QEMU here: choosing between "import this pristine cloud image" and "chain onto this
//! already-baked layer" happens entirely before the pipeline boots anything, and it is the
//! step that decided a chain could only ever be one layer deep — `import_base_image`
//! rejects a delta, so handing an already-prepared layer to `--base-image` failed outright.

use clap::Parser;
use pretty_assertions::assert_eq;
use std::path::{Path, PathBuf};
use tddy_vm::VmLibrary;
use tddy_vm_build::{resolve_layer_parent, Cli, CloudInitBuildArgs, Command};
use tempfile::TempDir;

/// An initialised library, plus the temp dir keeping it alive.
fn a_library() -> (TempDir, VmLibrary) {
    let dir = TempDir::new().expect("a temp dir must be creatable");
    let library = VmLibrary::new(dir.path().join("library"));
    library.init().expect("the library must initialise");
    (dir, library)
}

/// A pristine cloud image on disk — a whole qcow2 with no backing file, as a developer
/// downloads one.
///
/// Really created by `qemu-img`, tiny but genuine: the import inspects the format it was
/// handed, so a hand-written byte string would be rejected for the wrong reason.
fn a_pristine_cloud_image(dir: &Path) -> PathBuf {
    let path = dir.join("debian-12-genericcloud.qcow2");
    let created = std::process::Command::new("qemu-img")
        .args(["create", "-f", "qcow2"])
        .arg(&path)
        .arg("1M")
        .output()
        .expect("qemu-img must be on PATH");
    assert!(
        created.status.success(),
        "qemu-img create must succeed: {}",
        String::from_utf8_lossy(&created.stderr)
    );
    path
}

/// An already-baked layer sitting where a previous bake left it.
///
/// Its contents never matter: a parent layer is chained onto where it lies, so resolving it
/// is a path question and nothing opens the file.
fn a_prepared_layer(library: &VmLibrary, name: &str) -> PathBuf {
    let path = library.prepared_base_dir().join(format!("{name}.qcow2"));
    std::fs::write(&path, b"QFI\xfb-delta").expect("the prepared layer must be writable");
    path
}

/// Parse a `cloud-init` invocation, yielding its arguments or the error clap raised.
fn a_parsed_cloud_init_invocation(args: &[&str]) -> Result<CloudInitBuildArgs, clap::Error> {
    let argv = ["tddy-vm-build", "cloud-init"]
        .into_iter()
        .chain(args.iter().copied());
    match Cli::try_parse_from(argv)?.command {
        Command::CloudInit(args) => Ok(args),
        other => panic!("expected the cloud-init subcommand, got {other:?}"),
    }
}

#[test]
fn imports_a_pristine_base_image_into_the_libraries_first_layer() {
    // Given a library and a whole cloud image outside it
    let (dir, library) = a_library();
    let cloud_image = a_pristine_cloud_image(dir.path());

    // When the new layer names that image as its base
    let parent = resolve_layer_parent(&library, "tddy-nix-base", Some(&cloud_image), None)
        .expect("a pristine cloud image must be importable");

    // Then the layer chains onto the library's own copy, not the developer's path, so the
    // finished layer and its parent relocate together
    assert_eq!(
        parent,
        library.base_images_dir().join("tddy-nix-base.qcow2")
    );
}

#[test]
fn chains_onto_a_named_parent_layer_where_it_already_sits() {
    // Given a library holding an already-baked layer
    let (_dir, library) = a_library();
    let nix_base = a_prepared_layer(&library, "tddy-nix-base");

    // When a new layer names it as its parent
    let parent = resolve_layer_parent(&library, "tddy-builder", None, Some("tddy-nix-base"))
        .expect("an already-prepared layer must be chainable onto");

    // Then it is used where it lies: importing a delta into images/01-base/ would strand the
    // relative backing reference it resolves its own parent through
    assert_eq!(parent, nix_base);
}

#[test]
fn rejects_a_parent_layer_that_is_not_in_the_library() {
    // Given a library with nothing baked into it yet
    let (_dir, library) = a_library();

    // When a new layer names a parent that was never baked
    let error = resolve_layer_parent(&library, "tddy-builder", None, Some("tddy-nix-base"))
        .expect_err("a parent layer that does not exist must be refused");

    // Then the failure names the layer and where it was looked for
    assert_eq!(
        error.to_string(),
        format!(
            "no prepared layer named `tddy-nix-base` in {} — bake it first, or pass \
             --base-image to start a new chain from a pristine cloud image",
            library.prepared_base_dir().display()
        )
    );
}

#[test]
fn rejects_naming_both_a_base_image_and_a_parent_layer() {
    // Given a library, a cloud image, and an already-baked layer
    let (dir, library) = a_library();
    let cloud_image = a_pristine_cloud_image(dir.path());
    a_prepared_layer(&library, "tddy-nix-base");

    // When both are named as the new layer's parent
    let error = resolve_layer_parent(
        &library,
        "tddy-builder",
        Some(&cloud_image),
        Some("tddy-nix-base"),
    )
    .expect_err("naming two parents must be refused");

    // Then the failure says exactly one is expected, rather than silently preferring one
    assert_eq!(
        error.to_string(),
        "exactly one of --base-image or --parent-layer must be given"
    );
}

#[test]
fn rejects_naming_neither_a_base_image_nor_a_parent_layer() {
    // Given a library
    let (_dir, library) = a_library();

    // When neither a base image nor a parent layer is named
    let error = resolve_layer_parent(&library, "tddy-builder", None, None)
        .expect_err("naming no parent at all must be refused");

    // Then the failure says exactly one is expected
    assert_eq!(
        error.to_string(),
        "exactly one of --base-image or --parent-layer must be given"
    );
}

#[test]
fn the_cli_accepts_a_parent_layer_in_place_of_a_base_image() {
    // Given an invocation that chains onto an already-baked layer
    let argv = [
        "--name",
        "tddy-builder",
        "--parent-layer",
        "tddy-nix-base",
        "--user-data",
        "builder.yaml",
    ];

    // When it is parsed
    let args = a_parsed_cloud_init_invocation(&argv).expect("the invocation must parse");

    // Then the parent layer is carried through and no base image is implied
    assert_eq!(
        (args.base_image, args.parent_layer),
        (None, Some("tddy-nix-base".to_string()))
    );
}

#[test]
fn the_cli_rejects_an_invocation_naming_both_a_base_image_and_a_parent_layer() {
    // Given an invocation naming two different parents
    let argv = [
        "--name",
        "tddy-builder",
        "--base-image",
        "/images/debian-12-genericcloud.qcow2",
        "--parent-layer",
        "tddy-nix-base",
        "--user-data",
        "builder.yaml",
    ];

    // When it is parsed
    let error = a_parsed_cloud_init_invocation(&argv).expect_err("the invocation must be refused");

    // Then clap reports the two as mutually exclusive
    assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn the_cli_rejects_an_invocation_naming_no_parent_at_all() {
    // Given an invocation that names neither
    let argv = ["--name", "tddy-builder", "--user-data", "builder.yaml"];

    // When it is parsed
    let error = a_parsed_cloud_init_invocation(&argv).expect_err("the invocation must be refused");

    // Then clap reports the missing choice, rather than the command failing hours later
    assert_eq!(
        error.kind(),
        clap::error::ErrorKind::MissingRequiredArgument
    );
}

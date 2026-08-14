//! Guards the workspace against implicit qcow2 chain flattening.
//!
//! Flattening is `qemu-img convert` with a **qcow2 source**: it reads the whole backing
//! chain and writes one standalone image, silently turning a cheap delta into a full copy
//! and severing the layer's parent. Converting a **raw** source is a different thing
//! entirely — format normalisation of an image that never had a chain — and is allowed.
//!
//! This runs in the default suite because the property it protects is a property of the
//! source tree, not of a running VM: it costs milliseconds and it is the only thing
//! standing between a future edit and a silent return to full copies per layer.

use std::path::{Path, PathBuf};

/// The workspace root, from this crate's own compile-time manifest directory.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate must live two levels below the workspace root")
        .to_path_buf()
}

/// Every `.rs` file under `packages/*/src/`, i.e. production code only. Tests are excluded
/// deliberately: a test may legitimately construct a flattening argv to assert that
/// something rejects it.
fn production_sources() -> Vec<PathBuf> {
    let packages = workspace_root().join("packages");
    let mut sources = Vec::new();
    let entries = std::fs::read_dir(&packages).expect("packages/ must be readable");
    for entry in entries {
        let src = entry
            .expect("a packages/ entry must be readable")
            .path()
            .join("src");
        if src.is_dir() {
            collect_rust_files(&src, &mut sources);
        }
    }
    sources.sort();
    sources
}

fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).expect("a source directory must be readable");
    for entry in entries {
        let path = entry.expect("a source entry must be readable").path();
        if path.is_dir() {
            collect_rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Sources that convert a **raw** image into qcow2 — format normalisation of a source that
/// never had a backing chain, which is not flattening.
///
/// Listed by repo-relative path so that adding a new converter is a deliberate act with a
/// justification attached, rather than something that slips in unnoticed.
const RAW_TO_QCOW2_NORMALISERS: &[&str] = &[
    // Buildroot emits a raw ext2/ext4 rootfs; qcow2 is the packaging step.
    "packages/tddy-vm/src/build.rs",
    // The `qemu_disk_image` BUILD.yaml target, same raw rootfs, same reason.
    "packages/tddy-build-qemu/src/lib.rs",
    // Normalises a supplied non-qcow2 base image (raw, VMDK, VDI) into qcow2 on its way into
    // `images/01-base/`, and is also where the refusal that makes the qcow2-source convert
    // unrepresentable lives — so it names both formats deliberately.
    "packages/tddy-vm/src/image_import.rs",
];

fn repo_relative(path: &Path) -> String {
    path.strip_prefix(workspace_root())
        .expect("every source must live under the workspace root")
        .display()
        .to_string()
}

#[test]
fn no_production_code_converts_a_qcow2_source_into_a_standalone_image() {
    // Given every production source file in the workspace
    let sources = production_sources();

    // When each is inspected for a qcow2-to-qcow2 convert
    let offenders: Vec<String> = sources
        .iter()
        .filter(|path| {
            let relative = repo_relative(path);
            !RAW_TO_QCOW2_NORMALISERS.contains(&relative.as_str())
        })
        .filter(|path| {
            let text = std::fs::read_to_string(path).expect("a source file must be readable");
            // The argv shape `-f qcow2 ... -O qcow2` is the flattening signature. A raw
            // source reads `-f raw`, which this deliberately does not match.
            text.contains("\"convert\"") && text.contains("\"-f\"") && text.contains("\"qcow2\"")
        })
        .map(|path| repo_relative(path))
        .collect();

    // Then none exists. A qcow2 source means a chain to read and discard, which turns a
    // layer that should hold only its own delta into a full copy of everything beneath it
    assert_eq!(
        offenders,
        Vec::<String>::new(),
        "these files convert a qcow2 source, which flattens its backing chain"
    );
}

#[test]
fn the_only_convert_of_a_qcow2_source_left_in_the_workspace_is_none() {
    // Given the flattening argv builder that every flattening path used to route through

    // When production code is searched for it
    let survivors: Vec<String> = production_sources()
        .iter()
        .filter(|path| {
            let text = std::fs::read_to_string(path).expect("a source file must be readable");
            text.contains("base_convert_argv")
        })
        .map(|path| repo_relative(path))
        .collect();

    // Then it is gone entirely, rather than merely unused — an unused flattener is an
    // invitation, and this one's doc comment advertised the behaviour as a feature
    assert_eq!(
        survivors,
        Vec::<String>::new(),
        "base_convert_argv still exists; it flattens any backing chain on its source"
    );
}

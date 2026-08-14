//! Unit tests for `tddy_vm::image_import` — detecting what a supplied image actually is,
//! normalising a non-qcow2 one, and the refusal that keeps a backing chain from being
//! flattened by a `qemu-img convert`.

use std::path::{Path, PathBuf};
use tddy_vm::image_import::{
    normalise_to_qcow2, refuse_chain_flattening, supplied_image_format, SuppliedImageFormat,
};
use tempfile::tempdir;

/// A real, whole qcow2 image at `<dir>/<basename>.qcow2`.
fn a_qcow2_in(dir: &Path, basename: &str) -> PathBuf {
    let path = dir.join(format!("{basename}.qcow2"));
    let output = std::process::Command::new("qemu-img")
        .args(["create", "-f", "qcow2", path.to_str().unwrap(), "64M"])
        .output()
        .expect("qemu-img must be on PATH");
    assert!(
        output.status.success(),
        "qemu-img create failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    path
}

/// A real raw disk image at `<dir>/<basename>.img`.
fn a_raw_image_in(dir: &Path, basename: &str) -> PathBuf {
    let path = dir.join(format!("{basename}.img"));
    std::fs::write(&path, vec![0x01; 1024 * 1024]).unwrap();
    path
}

/// The first four bytes of every qcow2 image.
const QCOW2_MAGIC: &[u8] = b"QFI\xfb";

// ── supplied_image_format ─────────────────────────────────────────────────────

#[test]
fn a_qcow2_source_is_reported_as_needing_no_normalisation() {
    // Given a whole qcow2 image
    let dir = tempdir().unwrap();
    let src = a_qcow2_in(dir.path(), "cloud-image");

    // When its format is detected
    let format = supplied_image_format(&src).unwrap();

    // Then it is the one format the library stores as-is
    assert_eq!(format, SuppliedImageFormat::Qcow2);
}

#[test]
fn a_raw_source_is_reported_under_the_format_name_qemu_img_gives_it() {
    // Given a raw disk image
    let dir = tempdir().unwrap();
    let src = a_raw_image_in(dir.path(), "cloud-image");

    // When its format is detected
    let format = supplied_image_format(&src).unwrap();

    // Then it is named exactly as qemu-img names it, so the convert can pass it back
    assert_eq!(format, SuppliedImageFormat::Other("raw".to_string()));
}

#[test]
fn a_source_that_is_not_there_is_an_error_naming_it() {
    // Given a path with no image at it
    let dir = tempdir().unwrap();
    let missing = dir.path().join("absent.qcow2");

    // When its format is detected
    let message = supplied_image_format(&missing).unwrap_err().to_string();

    // Then the failure names the file, rather than a bare qemu-img diagnostic
    assert!(
        message.contains(missing.to_str().unwrap()),
        "the error must name the source it could not read, got: {message}"
    );
}

// ── normalise_to_qcow2 ────────────────────────────────────────────────────────

#[test]
fn normalising_a_raw_source_writes_a_qcow2_at_the_destination() {
    // Given a raw disk image and a destination beside it
    let dir = tempdir().unwrap();
    let src = a_raw_image_in(dir.path(), "cloud-image");
    let dest = dir.path().join("normalised.qcow2");

    // When it is normalised
    normalise_to_qcow2(&src, "raw", &dest).unwrap();

    // Then the destination is a qcow2
    assert_eq!(
        supplied_image_format(&dest).unwrap(),
        SuppliedImageFormat::Qcow2
    );
    assert_eq!(
        &std::fs::read(&dest).unwrap()[..QCOW2_MAGIC.len()],
        QCOW2_MAGIC
    );
}

#[test]
fn normalising_a_qcow2_source_is_refused_because_it_would_flatten_the_chain() {
    // Given a qcow2 source, which needs no normalisation and may sit on a backing chain
    let dir = tempdir().unwrap();
    let src = a_qcow2_in(dir.path(), "cloud-image");
    let dest = dir.path().join("flattened.qcow2");

    // When it is asked to be converted anyway
    let message = normalise_to_qcow2(&src, "qcow2", &dest)
        .unwrap_err()
        .to_string();

    // Then it is refused before qemu-img runs, and nothing is written: a qcow2 source means
    // a chain to read and discard, turning a delta into a full copy of everything beneath it
    assert!(
        message.contains("qcow2"),
        "the error must name the argv it refused, got: {message}"
    );
    assert!(
        !dest.exists(),
        "nothing must be written for a refused convert"
    );
}

// ── refuse_chain_flattening ───────────────────────────────────────────────────

#[test]
fn a_convert_of_a_qcow2_source_is_refused_naming_the_argv() {
    // Given the argv that flattens a backing chain
    let args = as_argv(&[
        "convert",
        "-f",
        "qcow2",
        "-O",
        "qcow2",
        "/in.qcow2",
        "/out.qcow2",
    ]);

    // When it is checked
    let message = refuse_chain_flattening(&args).unwrap_err();

    // Then it is refused, naming the argv so the caller can see which one it was
    assert_eq!(
        message,
        "refusing qemu-img [\"convert\", \"-f\", \"qcow2\", \"-O\", \"qcow2\", \"/in.qcow2\", \
         \"/out.qcow2\"]: converting a qcow2 source reads its whole backing chain and writes \
         one standalone image, silently turning a delta into a full copy and severing its parent"
    );
}

#[test]
fn a_convert_that_names_no_source_format_is_refused_because_qemu_would_probe_it() {
    // Given a convert argv that leaves the source format to qemu-img's own probing
    let args = as_argv(&["convert", "-O", "qcow2", "/in.img", "/out.qcow2"]);

    // When it is checked
    let message = refuse_chain_flattening(&args).unwrap_err();

    // Then it is refused too: a probed source may well be a qcow2, so an unnamed input
    // format is a flattening that has merely not been written down
    assert_eq!(
        message,
        "refusing qemu-img [\"convert\", \"-O\", \"qcow2\", \"/in.img\", \"/out.qcow2\"]: a \
         convert must name its input format with -f, since a probed source may be a qcow2 \
         whose backing chain would be flattened"
    );
}

#[test]
fn a_convert_of_a_raw_source_is_allowed() {
    // Given the argv that normalises a raw image into qcow2 — a source that never had a chain
    let args = as_argv(&[
        "convert",
        "-f",
        "raw",
        "-O",
        "qcow2",
        "/in.img",
        "/out.qcow2",
    ]);

    // When it is checked
    let result = refuse_chain_flattening(&args);

    // Then it is allowed
    assert_eq!(result, Ok(()));
}

#[test]
fn creating_an_overlay_backed_by_a_qcow2_parent_is_allowed() {
    // Given the overlay-create argv, which names qcow2 twice and flattens nothing
    let args = as_argv(&[
        "create",
        "-f",
        "qcow2",
        "-F",
        "qcow2",
        "-b",
        "/parent.qcow2",
        "/child.qcow2",
        "20G",
    ]);

    // When it is checked
    let result = refuse_chain_flattening(&args);

    // Then it is allowed
    assert_eq!(result, Ok(()));
}

fn as_argv(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| a.to_string()).collect()
}

//! Unit tests for `tddy_vm::library` — the VM & Image Library's directory layout,
//! base-image import with read-only protection, and the absolute-backing overlay argv.
//! Fails until `VmLibrary`'s methods and `vm_overlay_create_argv` are implemented.

use std::path::PathBuf;
use tddy_vm::library::{vm_overlay_create_argv, VmLibrary};
use tempfile::tempdir;

// ── Layout accessors ─────────────────────────────────────────────────────────

#[test]
fn accessors_resolve_the_01_base_02_prepared_base_and_per_vm_directories_under_the_root() {
    // Given a library rooted at a fixed path
    let library = VmLibrary::new(PathBuf::from("/data/.tddy"));

    // When resolving each library path
    // Then each matches the documented layout exactly
    assert_eq!(
        library.base_images_dir(),
        PathBuf::from("/data/.tddy/images/01-base")
    );
    assert_eq!(
        library.prepared_base_dir(),
        PathBuf::from("/data/.tddy/images/02-prepared-base")
    );
    assert_eq!(library.vms_dir(), PathBuf::from("/data/.tddy/vm"));
    assert_eq!(library.vm_dir("web"), PathBuf::from("/data/.tddy/vm/web"));
}

// ── init ──────────────────────────────────────────────────────────────────────

#[test]
fn init_creates_the_full_directory_tree() {
    // Given a fresh, empty root
    let dir = tempdir().unwrap();
    let library = VmLibrary::new(dir.path());

    // When init is called
    library.init().unwrap();

    // Then both image directories and the vm directory exist
    assert!(library.base_images_dir().is_dir());
    assert!(library.prepared_base_dir().is_dir());
    assert!(library.vms_dir().is_dir());
}

// ── import_base_image ─────────────────────────────────────────────────────────

/// An initialized library rooted at `dir`.
fn a_library_in(dir: &tempfile::TempDir) -> VmLibrary {
    let library = VmLibrary::new(dir.path());
    library.init().unwrap();
    library
}

/// A real, whole qcow2 image of `size` at `<dir>/<basename>.qcow2` — the shape a supplied
/// cloud image has, and the one `images/01-base/` is allowed to hold.
fn a_whole_qcow2_in(dir: &std::path::Path, basename: &str, size: &str) -> PathBuf {
    let path = dir.join(format!("{basename}.qcow2"));
    run_qemu_img(&["create", "-f", "qcow2", path.to_str().unwrap(), size]);
    path
}

/// A real raw disk image at `<dir>/<basename>.img`, every byte set to `fill` — a supplied
/// image in a format `images/01-base/` cannot hold as-is. Two of these with different fills
/// are the same length, so only a content comparison tells them apart.
fn a_raw_image_in(dir: &std::path::Path, basename: &str, fill: u8) -> PathBuf {
    let path = dir.join(format!("{basename}.img"));
    std::fs::write(&path, vec![fill; 1024 * 1024]).unwrap();
    path
}

/// The first four bytes of every qcow2 image.
const QCOW2_MAGIC: &[u8] = b"QFI\xfb";

#[cfg(unix)]
fn inode_of(path: &std::path::Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).unwrap().ino()
}

#[test]
fn import_base_image_copies_the_source_into_01_base_under_the_given_name() {
    // Given an initialized library and a whole qcow2 source
    let dir = tempdir().unwrap();
    let library = a_library_in(&dir);
    let src = a_whole_qcow2_in(dir.path(), "source", "64M");

    // When the source is imported as "debian-12"
    let stored = library.import_base_image(&src, "debian-12").unwrap();

    // Then it lands at images/01-base/debian-12.qcow2 with the same content
    assert_eq!(stored, library.base_images_dir().join("debian-12.qcow2"));
    assert_eq!(
        std::fs::read(&stored).unwrap(),
        std::fs::read(&src).unwrap()
    );
}

#[test]
fn import_base_image_locks_the_stored_file_read_only() {
    // Given an initialized library and a whole qcow2 source
    let dir = tempdir().unwrap();
    let library = a_library_in(&dir);
    let src = a_whole_qcow2_in(dir.path(), "source", "64M");

    // When the source is imported
    let stored = library.import_base_image(&src, "debian-12").unwrap();

    // Then the stored file is locked read-only (0o444) — protecting the immutable base
    // from accidental mutation. Unix-only: file mode bits have no equivalent on Windows.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&stored).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o444, "expected mode 0o444, got {mode:o}");
    }
}

#[test]
fn re_importing_the_identical_image_under_the_same_name_keeps_the_stored_content() {
    // Given a library that already has "debian-12" imported
    let dir = tempdir().unwrap();
    let library = a_library_in(&dir);
    let src = a_whole_qcow2_in(dir.path(), "source", "64M");
    library.import_base_image(&src, "debian-12").unwrap();

    // When the very same image is imported again under that name
    let stored = library.import_base_image(&src, "debian-12").unwrap();

    // Then the import succeeds and the stored base still holds that image
    assert_eq!(
        std::fs::read(&stored).unwrap(),
        std::fs::read(&src).unwrap()
    );
}

#[cfg(unix)]
#[test]
fn re_importing_the_identical_image_leaves_the_stored_file_in_place_rather_than_rewriting_it() {
    // Given a library that already has "debian-12" imported
    let dir = tempdir().unwrap();
    let library = a_library_in(&dir);
    let src = a_whole_qcow2_in(dir.path(), "source", "64M");
    let first = library.import_base_image(&src, "debian-12").unwrap();
    let inode_before = inode_of(&first);

    // When the very same image is imported again under that name
    let stored = library.import_base_image(&src, "debian-12").unwrap();

    // Then it is the same file on disk, not a replacement: an overlay records its parent by
    // path, so re-creating that file underneath a running chain is worth avoiding even when
    // the bytes agree. The inode is the only thing that tells a no-op from a re-copy.
    assert_eq!(inode_of(&stored), inode_before);
}

#[test]
fn importing_a_different_image_under_a_name_already_in_01_base_is_refused() {
    // Given a library that already has "debian-12" imported
    let dir = tempdir().unwrap();
    let library = a_library_in(&dir);
    let first_src = a_whole_qcow2_in(dir.path(), "first", "64M");
    let stored = library.import_base_image(&first_src, "debian-12").unwrap();
    let second_src = a_whole_qcow2_in(dir.path(), "second", "128M");

    // When a different image is imported under the same name
    let message = library
        .import_base_image(&second_src, "debian-12")
        .unwrap_err()
        .to_string();

    // Then it is refused, naming both images and why: qcow2 records no identity of its
    // parent, so silently swapping a base changes what every layer already chained onto it
    // sits on, and nothing downstream would notice
    assert!(
        message.contains(second_src.to_str().unwrap())
            && message.contains(stored.to_str().unwrap()),
        "the error must name both the source and the base it would have replaced, got: {message}"
    );
    assert!(
        message.contains("chained onto"),
        "the error must say why it was refused, got: {message}"
    );
}

#[test]
fn importing_a_different_image_under_an_existing_name_leaves_the_stored_base_untouched() {
    // Given a library that already has "debian-12" imported
    let dir = tempdir().unwrap();
    let library = a_library_in(&dir);
    let first_src = a_whole_qcow2_in(dir.path(), "first", "64M");
    let stored = library.import_base_image(&first_src, "debian-12").unwrap();
    let second_src = a_whole_qcow2_in(dir.path(), "second", "128M");

    // When a different image is imported under the same name
    library
        .import_base_image(&second_src, "debian-12")
        .unwrap_err();

    // Then the base every existing layer is chained onto is exactly as it was
    assert_eq!(
        std::fs::read(&stored).unwrap(),
        std::fs::read(&first_src).unwrap()
    );
}

// ── import_base_image: normalising a non-qcow2 source ─────────────────────────

#[test]
fn import_base_image_normalises_a_raw_source_into_a_qcow2() {
    // Given an initialized library and a raw disk image as the supplied base
    let dir = tempdir().unwrap();
    let library = a_library_in(&dir);
    let src = a_raw_image_in(dir.path(), "cloud-image", 0x01);

    // When it is imported
    let stored = library.import_base_image(&src, "debian-12").unwrap();

    // Then what lands in 01-base is a qcow2, not the raw bytes verbatim — every layer above
    // it is created with `-F qcow2`, which a raw parent fails much later and confusingly
    assert_eq!(
        &std::fs::read(&stored).unwrap()[..QCOW2_MAGIC.len()],
        QCOW2_MAGIC
    );
}

#[test]
fn re_importing_the_identical_raw_source_is_accepted_rather_than_read_as_a_changed_base() {
    // Given a library that already has a raw-sourced "debian-12" imported
    let dir = tempdir().unwrap();
    let library = a_library_in(&dir);
    let src = a_raw_image_in(dir.path(), "cloud-image", 0x01);
    let first = library.import_base_image(&src, "debian-12").unwrap();
    let normalised = std::fs::read(&first).unwrap();

    // When the same raw source is imported again under that name
    let stored = library.import_base_image(&src, "debian-12").unwrap();

    // Then it is a no-op on the normalised image, not a refusal: the comparison is against
    // what this source normalises to, not against the source's own raw bytes
    assert_eq!(std::fs::read(&stored).unwrap(), normalised);
}

#[test]
fn importing_a_different_image_of_the_very_same_size_is_still_refused() {
    // Given a library holding a base normalised from a raw image, and a second raw image of
    // exactly the same length but different content
    let dir = tempdir().unwrap();
    let library = a_library_in(&dir);
    let first_src = a_raw_image_in(dir.path(), "first", 0x01);
    library.import_base_image(&first_src, "debian-12").unwrap();
    let second_src = a_raw_image_in(dir.path(), "second", 0x02);

    // When the second is imported under the same name
    let message = library
        .import_base_image(&second_src, "debian-12")
        .unwrap_err()
        .to_string();

    // Then it is refused all the same — a size comparison would have called these identical,
    // which is precisely the swap that leaves every layer above chained onto other bytes
    assert!(
        message.contains("chained onto"),
        "the error must say why it was refused, got: {message}"
    );
}

/// A real qcow2 delta in `dir`, chained onto a real standalone parent beside it — the shape
/// every prepared base in `images/02-prepared-base/` has.
fn a_chained_qcow2_in(dir: &std::path::Path) -> PathBuf {
    let parent = dir.join("parent.qcow2");
    run_qemu_img(&["create", "-f", "qcow2", parent.to_str().unwrap(), "64M"]);
    let delta = dir.join("delta.qcow2");
    let args = vm_overlay_create_argv(&parent, &delta, "64M");
    run_qemu_img(&args.iter().map(String::as_str).collect::<Vec<_>>());
    delta
}

fn run_qemu_img(args: &[&str]) {
    let output = std::process::Command::new("qemu-img")
        .args(args)
        .output()
        .expect("qemu-img must be on PATH");
    assert!(
        output.status.success(),
        "qemu-img {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn import_base_image_rejects_a_qcow2_that_is_only_a_delta_of_another_image() {
    // Given an initialized library and a source that is a delta chained onto a parent
    let dir = tempdir().unwrap();
    let library = VmLibrary::new(dir.path());
    library.init().unwrap();
    let chained_src = a_chained_qcow2_in(dir.path());

    // When it is imported
    let result = library.import_base_image(&chained_src, "debian-12");

    // Then it is refused by name, rather than copied to a place its relative reference to
    // the parent no longer resolves from — 01-base holds whole images only
    let message = result.unwrap_err().to_string();
    assert!(
        message.contains(chained_src.to_str().unwrap()),
        "the error must name the source it refused, got: {message}"
    );
    assert!(
        message.contains("backing file"),
        "the error must say why it was refused, got: {message}"
    );
    assert!(
        !library.base_images_dir().join("debian-12.qcow2").exists(),
        "nothing must be imported when the source is refused"
    );
}

// ── vm_overlay_create_argv ────────────────────────────────────────────────────

#[test]
fn vm_overlay_create_argv_uses_an_absolute_backing_path_to_the_prepared_base() {
    // Given an absolute prepared-base path in the read-only library directory and a
    // per-VM overlay destination
    let prepared_base = PathBuf::from("/data/.tddy/images/02-prepared-base/debian-12.qcow2");
    let overlay = PathBuf::from("/data/.tddy/vm/web/web.qcow2");

    // When building the overlay-create argv
    let args = vm_overlay_create_argv(&prepared_base, &overlay, "20G");

    // Then it matches `qemu-img create -f qcow2 -F qcow2 -b <absolute-path> <overlay>
    // <size>` exactly — an absolute path, unlike cloud-init's co-located relative
    // basename (`overlay_create_argv`)
    assert_eq!(
        args,
        vec![
            "create".to_string(),
            "-f".to_string(),
            "qcow2".to_string(),
            "-F".to_string(),
            "qcow2".to_string(),
            "-b".to_string(),
            "/data/.tddy/images/02-prepared-base/debian-12.qcow2".to_string(),
            "/data/.tddy/vm/web/web.qcow2".to_string(),
            "20G".to_string(),
        ]
    );
}

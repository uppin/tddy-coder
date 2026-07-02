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

#[test]
fn import_base_image_copies_the_source_into_01_base_under_the_given_name() {
    // Given an initialized library and a source qcow2 file
    let dir = tempdir().unwrap();
    let library = VmLibrary::new(dir.path());
    library.init().unwrap();
    let src = dir.path().join("source.qcow2");
    std::fs::write(&src, b"fake qcow2 bytes").unwrap();

    // When the source is imported as "debian-12"
    let stored = library.import_base_image(&src, "debian-12").unwrap();

    // Then it lands at images/01-base/debian-12.qcow2 with the same content
    assert_eq!(stored, library.base_images_dir().join("debian-12.qcow2"));
    assert_eq!(std::fs::read(&stored).unwrap(), b"fake qcow2 bytes");
}

#[test]
fn import_base_image_locks_the_stored_file_read_only() {
    // Given an initialized library and a source qcow2 file
    let dir = tempdir().unwrap();
    let library = VmLibrary::new(dir.path());
    library.init().unwrap();
    let src = dir.path().join("source.qcow2");
    std::fs::write(&src, b"fake qcow2 bytes").unwrap();

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
fn import_base_image_replaces_an_existing_file_at_the_same_name() {
    // Given a library that already has a "debian-12" base imported
    let dir = tempdir().unwrap();
    let library = VmLibrary::new(dir.path());
    library.init().unwrap();
    let first_src = dir.path().join("first.qcow2");
    std::fs::write(&first_src, b"old bytes").unwrap();
    library.import_base_image(&first_src, "debian-12").unwrap();

    // When a different source is imported under the same name (unlock-before-overwrite)
    let second_src = dir.path().join("second.qcow2");
    std::fs::write(&second_src, b"new bytes").unwrap();
    let stored = library.import_base_image(&second_src, "debian-12").unwrap();

    // Then the stored file now holds the new content
    assert_eq!(std::fs::read(&stored).unwrap(), b"new bytes");
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

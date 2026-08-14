//! A layered image chain is a chain, not a pile of copies.
//!
//! These drive real `qemu-img` but never boot a guest, so they run in the default suite:
//! creating and inspecting an empty qcow2 costs milliseconds. What they pin is the
//! property the whole VM & Image Library rests on — that a layer holds **only its own
//! delta** and keeps a live reference to its parent.
//!
//! The distinction matters in bytes as well as design: under flattening, a test-host layer
//! adding one group and one directory cost a full copy of Debian plus a Nix store. As a
//! delta it costs tens of kilobytes.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_vm::cloud_init::relative_backing_path;
use tddy_vm::library::vm_overlay_create_argv;

/// A library-shaped directory tree: `images/01-base/` and `images/02-prepared-base/`.
struct AnImageLibrary {
    root: PathBuf,
    _temp: tempfile::TempDir,
}

fn an_image_library() -> AnImageLibrary {
    let temp = tempfile::tempdir().expect("a temp dir must be creatable");
    let root = temp.path().to_path_buf();
    std::fs::create_dir_all(root.join("images/01-base")).expect("01-base must be creatable");
    std::fs::create_dir_all(root.join("images/02-prepared-base"))
        .expect("02-prepared-base must be creatable");
    AnImageLibrary { root, _temp: temp }
}

impl AnImageLibrary {
    fn base_dir(&self) -> PathBuf {
        self.root.join("images/01-base")
    }

    fn prepared_dir(&self) -> PathBuf {
        self.root.join("images/02-prepared-base")
    }

    /// The one full image: a standalone qcow2 with no backing file, standing in for the
    /// imported pristine cloud image.
    ///
    /// Fully preallocated, so it occupies its whole 64M the way a real cloud image occupies
    /// its whole size. A sparse, empty qcow2 is a few hundred kilobytes of metadata
    /// whatever its virtual size — which would make "a delta is cheaper than a copy"
    /// unmeasurable, since there would be nothing to copy.
    fn with_pristine_base(&self, name: &str) -> PathBuf {
        let path = self.base_dir().join(format!("{name}.qcow2"));
        run_qemu_img(
            &[
                "create".into(),
                "-f".into(),
                "qcow2".into(),
                "-o".into(),
                "preallocation=full".into(),
                path.display().to_string(),
                "64M".into(),
            ],
            &self.base_dir(),
        );
        path
    }

    /// A layer in `02-prepared-base/`, backed by `parent` through a relative reference and
    /// created **in its final location** — a relative backing path is resolved against the
    /// referencing image's own directory, so an overlay created elsewhere and moved would
    /// point at nothing.
    fn with_layer(&self, name: &str, parent: &Path) -> PathBuf {
        let overlay = self.prepared_dir().join(format!("{name}.qcow2"));
        let backing = relative_backing_path(&self.prepared_dir(), parent)
            .expect("the layer and its parent are both named absolutely");
        run_qemu_img(
            &vm_overlay_create_argv_relative(&backing, &overlay, "10G"),
            &self.prepared_dir(),
        );
        overlay
    }
}

/// `qemu-img create` argv backing an overlay onto a caller-supplied relative path.
fn vm_overlay_create_argv_relative(backing: &str, overlay: &Path, size: &str) -> Vec<String> {
    vec![
        "create".into(),
        "-f".into(),
        "qcow2".into(),
        "-F".into(),
        "qcow2".into(),
        "-b".into(),
        backing.into(),
        overlay.display().to_string(),
        size.into(),
    ]
}

fn run_qemu_img(args: &[String], cwd: &Path) {
    let output = std::process::Command::new("qemu-img")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("qemu-img must be on PATH");
    assert!(
        output.status.success(),
        "qemu-img {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The backing reference `qemu-img info` reports as recorded in the image, or `None` when
/// the image has no parent.
///
/// qemu-img annotates a relative reference with the absolute path it currently resolves to
/// (`../01-base/x.qcow2 (actual path: /tmp/…/../01-base/x.qcow2)`). That resolution is a
/// property of where the file happens to sit right now; what is recorded in the image is
/// the part before it, and that is what these tests are about.
fn backing_file_of(image: &Path) -> Option<String> {
    let output = std::process::Command::new("qemu-img")
        .args(["info", &image.display().to_string()])
        .output()
        .expect("qemu-img must be on PATH");
    assert!(output.status.success(), "qemu-img info must succeed");
    let text = String::from_utf8_lossy(&output.stdout);
    // `Option`, not a defaulted `String`: "this image has no parent" and "this helper
    // failed to find the line it was looking for" are different answers, and collapsing
    // them into `""` would make the no-parent assertion pass for either reason — including
    // if a future qemu-img reworded the line for every image.
    let reported = text
        .lines()
        .find_map(|line| line.strip_prefix("backing file: "))?;
    Some(
        reported
            .split(" (actual path:")
            .next()
            .unwrap_or(reported)
            .trim()
            .to_string(),
    )
}

/// The backing reference an image must have, failing loudly when it has none.
fn backing_file_naming_a_parent(image: &Path) -> String {
    backing_file_of(image).unwrap_or_else(|| panic!("{} must record a parent", image.display()))
}

/// Bytes the file actually occupies, which for a qcow2 delta is its own content only.
fn size_on_disk(image: &Path) -> u64 {
    std::fs::metadata(image)
        .expect("the image must exist")
        .len()
}

#[test]
fn records_the_pristine_import_as_the_only_image_without_a_parent() {
    // Given a library holding the imported pristine image
    let library = an_image_library();
    let pristine = library.with_pristine_base("debian-12");

    // When its backing file is read
    let backing = backing_file_of(&pristine);

    // Then it has none — it is the one full image every layer ultimately rests on
    assert_eq!(backing, None);
}

#[test]
fn backs_the_first_prepared_layer_onto_the_pristine_import_across_the_tier_boundary() {
    // Given a library with a pristine base
    let library = an_image_library();
    let pristine = library.with_pristine_base("debian-12");

    // When a prepared layer is created in 02-prepared-base
    let nix_base = library.with_layer("tddy-nix-base", &pristine);

    // Then it points back across the tier boundary by a relative path, so the whole
    // library can be relocated without rewriting a single backing reference
    assert_eq!(
        backing_file_naming_a_parent(&nix_base),
        "../01-base/debian-12.qcow2"
    );
}

#[test]
fn backs_a_sibling_layer_onto_its_parent_by_bare_filename() {
    // Given a library with a prepared parent layer
    let library = an_image_library();
    let pristine = library.with_pristine_base("debian-12");
    let nix_base = library.with_layer("tddy-nix-base", &pristine);

    // When a child layer is created beside it
    let builder = library.with_layer("tddy-builder", &nix_base);

    // Then the reference is a bare filename, because parent and child share a directory
    assert_eq!(
        backing_file_naming_a_parent(&builder),
        "tddy-nix-base.qcow2"
    );
}

#[test]
fn keeps_two_children_of_one_parent_as_separate_deltas_rather_than_two_full_copies() {
    // Given a parent layer with two children — the shape the testkit's builder and test
    // host form off one shared Nix parent
    let library = an_image_library();
    let pristine = library.with_pristine_base("debian-12");
    let nix_base = library.with_layer("tddy-nix-base", &pristine);
    let builder = library.with_layer("tddy-builder", &nix_base);
    let test_host = library.with_layer("tddy-test-host", &nix_base);

    // Then both name the same parent
    assert_eq!(
        backing_file_naming_a_parent(&builder),
        "tddy-nix-base.qcow2"
    );
    assert_eq!(
        backing_file_naming_a_parent(&test_host),
        "tddy-nix-base.qcow2"
    );

    // And each costs its own delta rather than a copy of everything beneath it. A freshly
    // created overlay is qcow2 metadata only, so it is far smaller than its 64M parent —
    // under flattening each of these would have been a full standalone image instead.
    assert!(
        size_on_disk(&builder) < size_on_disk(&pristine),
        "a delta must be smaller than the image it derives from: {} vs {}",
        size_on_disk(&builder),
        size_on_disk(&pristine)
    );
    assert!(
        size_on_disk(&test_host) < size_on_disk(&pristine),
        "a delta must be smaller than the image it derives from: {} vs {}",
        size_on_disk(&test_host),
        size_on_disk(&pristine)
    );
}

#[test]
fn resolves_the_whole_chain_after_the_library_root_is_moved() {
    // Given a three-deep chain in a library
    let library = an_image_library();
    let pristine = library.with_pristine_base("debian-12");
    let nix_base = library.with_layer("tddy-nix-base", &pristine);
    library.with_layer("tddy-builder", &nix_base);

    // When the entire library is relocated, as it is when a checkout moves — `tmp/.tddy`
    // is repo-relative
    let destination = tempfile::tempdir().expect("a temp dir must be creatable");
    let moved_root = destination.path().join("relocated");
    std::fs::rename(&library.root, &moved_root).expect("the library must be movable");

    // Then the chain still resolves end to end, which is the entire reason the references
    // are relative rather than absolute
    let moved_builder = moved_root.join("images/02-prepared-base/tddy-builder.qcow2");
    let output = std::process::Command::new("qemu-img")
        .args(["check", &moved_builder.display().to_string()])
        .output()
        .expect("qemu-img must be on PATH");
    assert!(
        output.status.success(),
        "the relocated chain must still resolve: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn leaves_every_ancestor_byte_for_byte_unchanged_when_a_child_is_added() {
    // Given a chain whose ancestors have known contents
    let library = an_image_library();
    let pristine = library.with_pristine_base("debian-12");
    let nix_base = library.with_layer("tddy-nix-base", &pristine);
    let pristine_before = std::fs::read(&pristine).expect("the pristine image must be readable");
    let nix_base_before = std::fs::read(&nix_base).expect("the parent layer must be readable");

    // When a child is created against the parent
    library.with_layer("tddy-builder", &nix_base);

    // Then neither ancestor is touched. Layers are sealed 0444 for this reason: a parent
    // mutated after its children exist corrupts every delta that depends on it, and
    // nothing in the qcow2 format detects that
    assert_eq!(
        std::fs::read(&pristine).expect("the pristine image must still be readable"),
        pristine_before
    );
    assert_eq!(
        std::fs::read(&nix_base).expect("the parent layer must still be readable"),
        nix_base_before
    );
}

/// The one place `vm_overlay_create_argv` is still exercised for its absolute-path shape,
/// so removing flattening does not silently change the per-VM overlay path too.
#[test]
fn still_backs_a_per_vm_overlay_onto_an_absolute_prepared_base_path() {
    // Given a prepared base and a per-VM overlay in a different directory
    let prepared = Path::new("/lib/images/02-prepared-base/tddy-test-host.qcow2");
    let overlay = Path::new("/lib/vm/tddy-test-42/tddy-test-42.qcow2");

    // When the per-VM overlay argv is built
    let argv = vm_overlay_create_argv(prepared, overlay, "40G");

    // Then it still names the parent absolutely. Per-VM overlays are disposable and never
    // relocated, so they do not need the relative discipline the library layers use
    assert_eq!(
        argv.join(" "),
        "create -f qcow2 -F qcow2 -b /lib/images/02-prepared-base/tddy-test-host.qcow2 \
         /lib/vm/tddy-test-42/tddy-test-42.qcow2 40G"
    );
}

//! Unit: a `mirror` item lowered into a home an older launch already seeded.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-jail-from-spec.md` (`#agent-tools 5/12`).
//!
//! The recipes this replaces seeded `.local/share/claude/versions/<version>` as a real directory of
//! links, and the persistent home that holds it is migrated, not recreated. The mirror of that
//! directory must neither fail the jail's start (a link cannot be made over a directory) nor
//! delete anything but the old links.

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_actions::tool::FileItem;
use tddy_sandbox::SymlinkSpec;
use tddy_sandbox_recipes::{lower_files, JailPaths};

const VERSIONS: &str = ".local/share/claude/versions";

struct ALayout {
    _root: tempfile::TempDir,
    home: PathBuf,
    host_home: PathBuf,
    tmp: PathBuf,
}

fn a_layout_with_host_versions() -> ALayout {
    let root = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(root.path()).unwrap();
    let layout = ALayout {
        home: base.join("homes/claude"),
        host_home: base.join("host"),
        tmp: base.join("tmp"),
        _root: root,
    };
    std::fs::create_dir_all(layout.host_home.join(VERSIONS)).unwrap();
    std::fs::create_dir_all(&layout.home).unwrap();
    layout
}

fn lowered_mirror(layout: &ALayout) -> std::io::Result<Vec<SymlinkSpec>> {
    let paths = JailPaths {
        tddy_home: layout.home.parent().unwrap(),
        session_home: &layout.home,
        tmp: &layout.tmp,
        host_home: &layout.host_home,
    };
    let item = FileItem::Mirror {
        from_host: PathBuf::from(VERSIONS),
        to: PathBuf::from(VERSIONS),
    };
    lower_files(&[item], &layout.home, &paths).map(|grants| grants.symlinks)
}

fn expected_link(layout: &ALayout) -> Vec<SymlinkSpec> {
    vec![SymlinkSpec {
        link: layout.home.join(VERSIONS),
        target: layout.host_home.join(VERSIONS),
    }]
}

#[test]
fn a_directory_of_links_an_older_launch_seeded_is_replaced_by_the_mirror() {
    // Given
    let layout = a_layout_with_host_versions();
    let old = layout.home.join(VERSIONS);
    std::fs::create_dir_all(&old).unwrap();
    symlink("/somewhere/claude-1.2.3", old.join("1.2.3")).unwrap();

    // When
    let symlinks = lowered_mirror(&layout);

    // Then
    assert_eq!(
        (symlinks.map_err(|e| e.to_string()), old.exists()),
        (Ok(expected_link(&layout)), false)
    );
}

#[test]
fn a_directory_holding_anything_but_links_is_left_alone_and_not_mirrored_over() {
    // Given
    let layout = a_layout_with_host_versions();
    let own = layout.home.join(VERSIONS);
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(own.join("notes.txt"), "the tool's own file").unwrap();

    // When
    let symlinks = lowered_mirror(&layout);

    // Then
    assert_eq!(
        (
            symlinks.map_err(|e| e.to_string()),
            read(&own.join("notes.txt"))
        ),
        (Ok(Vec::new()), "the tool's own file".to_string())
    );
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

//! The byte-exact worktree read — AC15-AC20 of `docs/ft/daemon/session-worktree-sync.md`.
//!
//! Exercises `read_worktree_file_bytes`, the reader the streaming RPC frames. The framing itself is
//! transport, tested where the transport is; what is pinned here is the thing the RPC would be
//! useless without: that the bytes come back unaltered, that an oversized file is refused rather
//! than shortened, and that every guard the unary reader applies is applied here too.

use std::path::Path;
use std::process::Command;

use pretty_assertions::assert_eq;
use tddy_daemon::worktree_files::{read_worktree_file_bytes, MAX_WORKTREE_FILE_BYTES};

/// Well above anything these fixtures write, so a size refusal in this suite is always the one the
/// test asked for.
const A_ROOMY_CAP: u64 = 64 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A worktree whose files are all tracked, since the git listing is what gates a read.
fn a_worktree_containing(root: &Path, files: &[(&str, &[u8])]) {
    git(root, &["init", "--initial-branch=main"]);
    git(root, &["config", "user.email", "agent@example.com"]);
    git(root, &["config", "user.name", "Agent"]);
    for (name, contents) in files {
        let path = root.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(&path, contents).expect("write fixture file");
    }
    git(root, &["add", "-A"]);
    git(root, &["commit", "-m", "fixture"]);
}

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("failed to run git {args:?}: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The status code of a refused read, for a test that is about the refusal.
fn refused(root: &Path, rel_path: &str, cap: u64) -> tddy_rpc::Code {
    match read_worktree_file_bytes(root, rel_path, cap) {
        Err(status) => status.code(),
        Ok(bytes) => panic!(
            "expected {rel_path} to be refused, got {} bytes",
            bytes.len()
        ),
    }
}

// ---------------------------------------------------------------------------
// Byte fidelity
// ---------------------------------------------------------------------------

#[test]
fn reads_a_png_byte_for_byte() {
    // Given a real PNG header followed by bytes no UTF-8 decoder accepts
    let repo = tempfile::tempdir().expect("tempdir");
    let png: Vec<u8> = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x80, 0xFF,
    ];
    a_worktree_containing(repo.path(), &[("logo.png", &png)]);

    // When
    let read = read_worktree_file_bytes(repo.path(), "logo.png", A_ROOMY_CAP).expect("must read");

    // Then — the unary reader answers FAILED_PRECONDITION for exactly this file.
    assert_eq!(read, png);
}

#[test]
fn reads_a_lone_continuation_byte_that_is_not_valid_utf8() {
    // Given a single 0x80, which is a continuation byte with nothing to continue
    let repo = tempfile::tempdir().expect("tempdir");
    a_worktree_containing(repo.path(), &[("odd.bin", &[0x80][..])]);

    // When
    let read = read_worktree_file_bytes(repo.path(), "odd.bin", A_ROOMY_CAP).expect("must read");

    // Then
    assert_eq!(read, vec![0x80]);
}

#[test]
fn reads_a_utf16_file_without_transcoding_it() {
    // Given UTF-16LE with a BOM — valid text in an encoding that is not UTF-8
    let repo = tempfile::tempdir().expect("tempdir");
    let utf16: Vec<u8> = vec![0xFF, 0xFE, 0x68, 0x00, 0x69, 0x00];
    a_worktree_containing(repo.path(), &[("notes.txt", &utf16)]);

    // When
    let read = read_worktree_file_bytes(repo.path(), "notes.txt", A_ROOMY_CAP).expect("must read");

    // Then the bytes are the file's own, not a re-encoding of them.
    assert_eq!(read, utf16);
}

#[test]
fn reads_a_file_larger_than_the_unary_readers_one_megabyte_cap_without_truncating_it() {
    // Given a file above MAX_WORKTREE_FILE_BYTES, which the unary reader would cut short
    let repo = tempfile::tempdir().expect("tempdir");
    let big = vec![b'x'; MAX_WORKTREE_FILE_BYTES + 4096];
    a_worktree_containing(repo.path(), &[("big.txt", &big)]);

    // When
    let read = read_worktree_file_bytes(repo.path(), "big.txt", A_ROOMY_CAP).expect("must read");

    // Then every byte comes back: a truncated mirror is a wrong mirror, and nothing downstream
    // could tell it from a correct one.
    assert_eq!(read.len(), MAX_WORKTREE_FILE_BYTES + 4096);
    assert_eq!(read, big);
}

#[test]
fn reads_an_empty_file_as_zero_bytes() {
    // Given an empty tracked file
    let repo = tempfile::tempdir().expect("tempdir");
    a_worktree_containing(repo.path(), &[("empty.txt", &[][..])]);

    // When
    let read = read_worktree_file_bytes(repo.path(), "empty.txt", A_ROOMY_CAP).expect("must read");

    // Then it is a successful read of nothing, not a failure.
    assert_eq!(read, Vec::<u8>::new());
}

// ---------------------------------------------------------------------------
// Refusals
// ---------------------------------------------------------------------------

#[test]
fn refuses_a_file_over_the_cap_rather_than_shortening_it() {
    // Given a file above the cap it is read under
    let repo = tempfile::tempdir().expect("tempdir");
    a_worktree_containing(repo.path(), &[("big.txt", &vec![b'x'; 4096])]);

    // When
    let code = refused(repo.path(), "big.txt", 1024);

    // Then it is refused, not cut: a caller reconstructing a file needs all of it or an error.
    assert_eq!(code, tddy_rpc::Code::InvalidArgument);
}

#[test]
fn refuses_a_path_that_climbs_out_of_the_worktree() {
    // Given a worktree
    let repo = tempfile::tempdir().expect("tempdir");
    a_worktree_containing(repo.path(), &[("README.md", b"one\n")]);

    // When a traversal is attempted
    let code = refused(repo.path(), "../../../etc/passwd", A_ROOMY_CAP);

    // Then
    assert_eq!(code, tddy_rpc::Code::InvalidArgument);
}

#[test]
fn refuses_an_absolute_path() {
    // Given a worktree
    let repo = tempfile::tempdir().expect("tempdir");
    a_worktree_containing(repo.path(), &[("README.md", b"one\n")]);

    // When
    let code = refused(repo.path(), "/etc/passwd", A_ROOMY_CAP);

    // Then
    assert_eq!(code, tddy_rpc::Code::InvalidArgument);
}

#[test]
fn refuses_a_file_git_does_not_list() {
    // Given a file present on disk but excluded from the listing
    let repo = tempfile::tempdir().expect("tempdir");
    a_worktree_containing(repo.path(), &[(".gitignore", b"secrets.txt\n")]);
    std::fs::write(repo.path().join("secrets.txt"), b"token\n").expect("write");

    // When
    let code = refused(repo.path(), "secrets.txt", A_ROOMY_CAP);

    // Then the git listing is the gate here exactly as it is on the unary reader — the two may
    // differ in what they return, never in what they allow.
    assert_eq!(code, tddy_rpc::Code::PermissionDenied);
}

#[test]
fn refuses_to_read_the_git_directory() {
    // Given a worktree
    let repo = tempfile::tempdir().expect("tempdir");
    a_worktree_containing(repo.path(), &[("README.md", b"one\n")]);

    // When
    let code = refused(repo.path(), ".git/config", A_ROOMY_CAP);

    // Then
    assert_eq!(code, tddy_rpc::Code::PermissionDenied);
}

#[cfg(unix)]
#[test]
fn refuses_a_symlink_that_resolves_outside_the_worktree() {
    // Given a tracked symlink pointing out of the worktree
    let repo = tempfile::tempdir().expect("tempdir");
    a_worktree_containing(repo.path(), &[("README.md", b"one\n")]);
    std::os::unix::fs::symlink("/etc/passwd", repo.path().join("escape")).expect("symlink");
    git(repo.path(), &["add", "escape"]);
    git(repo.path(), &["commit", "-m", "add symlink"]);

    // When
    let code = refused(repo.path(), "escape", A_ROOMY_CAP);

    // Then being listed by git is not enough — the resolved path must still be inside.
    assert_eq!(code, tddy_rpc::Code::PermissionDenied);
}

#[test]
fn reports_a_tracked_file_deleted_from_the_worktree_as_not_found() {
    // Given a file git still lists, deleted from disk but not yet staged
    let repo = tempfile::tempdir().expect("tempdir");
    a_worktree_containing(
        repo.path(),
        &[("README.md", b"one\n"), ("gone.txt", b"two\n")],
    );
    std::fs::remove_file(repo.path().join("gone.txt")).expect("delete");

    // When
    let code = refused(repo.path(), "gone.txt", A_ROOMY_CAP);

    // Then absence is worth reporting here, and reporting it leaks nothing — the caller has
    // already been told the path is one git lists.
    assert_eq!(code, tddy_rpc::Code::NotFound);
}

#[test]
fn refuses_an_unlisted_path_identically_whether_or_not_it_exists() {
    // Given two gitignored names, one present on disk and one absent
    let repo = tempfile::tempdir().expect("tempdir");
    a_worktree_containing(repo.path(), &[(".gitignore", b"secrets.txt\nabsent.txt\n")]);
    std::fs::write(repo.path().join("secrets.txt"), b"token\n").expect("write");

    // When each is asked for
    let present = refused(repo.path(), "secrets.txt", A_ROOMY_CAP);
    let absent = refused(repo.path(), "absent.txt", A_ROOMY_CAP);

    // Then the answers are the same. Differing here would keep the CONTENTS of an ignored file
    // secret while handing out its existence — probe `.env` and read the answer off the status
    // code — which is exactly what the listing gate exists to prevent.
    assert_eq!(present, tddy_rpc::Code::PermissionDenied);
    assert_eq!(absent, tddy_rpc::Code::PermissionDenied);
}

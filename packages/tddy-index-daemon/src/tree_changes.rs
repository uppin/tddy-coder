//! What changed on disk under a warm root between two of its requests, told to its server.
//!
//! A server this process keeps warm outlives every request, and the tree changes behind it between
//! them: an `apply` writes a module and its declaration, a developer hand-writes a file. The engine
//! closes every document it opened when an operation ends, which hands a file the server already
//! knew back to the disk — but that close lands *before* an `apply` writes, and it never names a
//! file the server has not seen. rust-analyzer's own watcher did not make up for it on this
//! repository's workspace (1,068 crates): a warm `check --deep` anchored in the module the previous
//! `apply` had created never answered, and a type in a hand-written file stayed `_` in an extracted
//! signature, while the same runs cold answered. Why that watcher is blind there is not established;
//! on a small fixture it sees both.
//!
//! So the host does what the protocol gives a client for exactly this: before a warm server is handed
//! to a request, it compares the tree with what it held when the previous request was handed the
//! server, and sends `workspace/didChangeWatchedFiles` naming every source file created, changed or
//! deleted since. rust-analyzer re-reads each one from disk; a document some request holds open is
//! unaffected, because an open document's text is the client's, not the disk's.
//!
//! **What is compared.** Every `*.rs`, `Cargo.toml` and `Cargo.lock` under the root, by modification
//! time and length. Directories named `target` or `node_modules`, and every hidden one (`.git`,
//! `.restructure`, nested `.worktrees`), are not entered; symbolic links are not followed. A walk of
//! this repository (about 1,850 such files) takes a fifth of a second, paid once per request.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::{json, Value};

/// The source files under one root, as the disk held them at one moment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TreeSnapshot {
    files: BTreeMap<PathBuf, Stamp>,
}

/// What a write to a file alters, as cheaply as it can be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    modified: SystemTime,
    len: u64,
}

/// The LSP `FileChangeType` a file's difference between two snapshots is announced as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FileChange {
    Created = 1,
    Changed = 2,
    Deleted = 3,
}

impl TreeSnapshot {
    /// Read every source file under `root`.
    ///
    /// A file or directory that vanishes while it is read is simply absent: the next snapshot says
    /// what it became. Any other failure to read the tree is returned, because a server told nothing
    /// about a directory this could not read would be served a tree nobody can vouch for.
    pub(crate) fn read(root: &Path) -> io::Result<Self> {
        let mut files = BTreeMap::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(directory) = pending.pop() {
            let entries = match std::fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            for entry in entries {
                let entry = entry?;
                let kind = entry.file_type()?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if kind.is_dir() {
                    if !is_skipped_directory(&name) {
                        pending.push(entry.path());
                    }
                    continue;
                }
                if !kind.is_file() || !is_source_file(&name) {
                    continue;
                }
                match entry.metadata() {
                    Ok(metadata) => {
                        files.insert(
                            entry.path(),
                            Stamp {
                                modified: metadata.modified()?,
                                len: metadata.len(),
                            },
                        );
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(Self { files })
    }

    /// Every file that differs from `earlier`, in path order.
    pub(crate) fn changes_since(&self, earlier: &TreeSnapshot) -> Vec<(PathBuf, FileChange)> {
        let mut changes: Vec<(PathBuf, FileChange)> = self
            .files
            .iter()
            .filter_map(|(path, stamp)| match earlier.files.get(path) {
                None => Some((path.clone(), FileChange::Created)),
                Some(before) if before != stamp => Some((path.clone(), FileChange::Changed)),
                Some(_) => None,
            })
            .chain(
                earlier
                    .files
                    .keys()
                    .filter(|path| !self.files.contains_key(*path))
                    .map(|path| (path.clone(), FileChange::Deleted)),
            )
            .collect();
        changes.sort_by(|left, right| left.0.cmp(&right.0));
        changes
    }
}

/// The `workspace/didChangeWatchedFiles` parameters announcing `changes`.
///
/// Addressed as `file://<path>`, the form the restructuring engine opens documents in, so the server
/// reads both as one file.
pub(crate) fn watched_files_params(changes: &[(PathBuf, FileChange)]) -> Value {
    json!({
        "changes": changes
            .iter()
            .map(|(path, change)| json!({
                "uri": format!("file://{}", path.display()),
                "type": *change as u8,
            }))
            .collect::<Vec<_>>()
    })
}

fn is_skipped_directory(name: &str) -> bool {
    name.starts_with('.') || name == "target" || name == "node_modules"
}

fn is_source_file(name: &str) -> bool {
    name.ends_with(".rs") || name == "Cargo.toml" || name == "Cargo.lock"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_tree() -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().expect("a temporary tree");
        let root = directory.path().canonicalize().expect("the root resolves");
        std::fs::create_dir_all(root.join("src")).expect("src");
        std::fs::write(root.join("Cargo.toml"), "[package]\n").expect("a manifest");
        std::fs::write(root.join("src/lib.rs"), "pub mod a;\n").expect("lib.rs");
        (directory, root)
    }

    fn write(root: &Path, relative: &str, text: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");
        std::fs::write(path, text).expect("the file");
    }

    #[test]
    fn reads_a_created_a_changed_and_a_deleted_source_file() {
        // Given a tree, and then one file written, one rewritten and one removed
        let (_directory, root) = a_tree();
        write(&root, "src/gone.rs", "fn gone() {}\n");
        let earlier = TreeSnapshot::read(&root).expect("the tree reads");
        write(&root, "src/a.rs", "pub fn a() {}\n");
        write(&root, "src/lib.rs", "pub mod a;\npub mod b;\n");
        std::fs::remove_file(root.join("src/gone.rs")).expect("removed");

        // When the tree is read again and compared
        let changes = TreeSnapshot::read(&root)
            .expect("the tree reads")
            .changes_since(&earlier);

        // Then each file is named once, as what happened to it
        assert_eq!(
            changes,
            vec![
                (root.join("src/a.rs"), FileChange::Created),
                (root.join("src/gone.rs"), FileChange::Deleted),
                (root.join("src/lib.rs"), FileChange::Changed),
            ]
        );
    }

    #[test]
    fn reads_no_change_in_a_build_directory_a_hidden_one_or_a_file_that_is_not_source() {
        // Given a tree, and then files written where no server reads source
        let (_directory, root) = a_tree();
        let earlier = TreeSnapshot::read(&root).expect("the tree reads");
        write(&root, "target/debug/build/out/generated.rs", "fn g() {}\n");
        write(&root, ".restructure/journal.rs", "fn j() {}\n");
        write(&root, ".worktrees/other/src/lib.rs", "fn o() {}\n");
        write(&root, "node_modules/x/index.rs", "fn n() {}\n");
        write(&root, "README.md", "# a crate\n");

        // When the tree is read again and compared
        let changes = TreeSnapshot::read(&root)
            .expect("the tree reads")
            .changes_since(&earlier);

        // Then nothing is named
        assert_eq!(changes, Vec::new());
    }

    #[test]
    fn reads_a_changed_manifest_and_lock_file() {
        // Given a tree, and then its manifest rewritten and a lock file written
        let (_directory, root) = a_tree();
        let earlier = TreeSnapshot::read(&root).expect("the tree reads");
        write(&root, "Cargo.toml", "[package]\nname = \"a\"\n");
        write(&root, "Cargo.lock", "version = 4\n");

        // When the tree is read again and compared
        let changes = TreeSnapshot::read(&root)
            .expect("the tree reads")
            .changes_since(&earlier);

        // Then both are named, since a server reloads the crate graph on either
        assert_eq!(
            changes,
            vec![
                (root.join("Cargo.lock"), FileChange::Created),
                (root.join("Cargo.toml"), FileChange::Changed),
            ]
        );
    }

    #[test]
    fn announces_each_change_as_the_protocol_s_file_change_type() {
        // Given one change of each kind
        let changes = vec![
            (PathBuf::from("/w/src/a.rs"), FileChange::Created),
            (PathBuf::from("/w/src/b.rs"), FileChange::Changed),
            (PathBuf::from("/w/src/c.rs"), FileChange::Deleted),
        ];

        // When they are made into the notification's parameters
        let params = watched_files_params(&changes);

        // Then each is a file URI with its `FileChangeType`
        assert_eq!(
            params,
            json!({ "changes": [
                { "uri": "file:///w/src/a.rs", "type": 1 },
                { "uri": "file:///w/src/b.rs", "type": 2 },
                { "uri": "file:///w/src/c.rs", "type": 3 },
            ]})
        );
    }
}

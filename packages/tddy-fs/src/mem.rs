//! The in-memory adapter, for tests.

use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::SystemTime;

use crate::{Fs, FsKind, FsMetadata};

/// What a new file carries when no mode was stated, as under a `0o022` umask.
const DEFAULT_FILE_MODE: u32 = 0o644;
/// What a new directory carries when created, as under a `0o022` umask.
const DEFAULT_DIR_MODE: u32 = 0o755;

#[derive(Debug, Clone)]
enum Node {
    File {
        bytes: Vec<u8>,
        mode: u32,
        modified: SystemTime,
    },
    Dir {
        mode: u32,
        modified: SystemTime,
    },
}

impl Node {
    fn kind(&self) -> FsKind {
        match self {
            Node::File { .. } => FsKind::File,
            Node::Dir { .. } => FsKind::Dir,
        }
    }

    fn mode(&self) -> u32 {
        match self {
            Node::File { mode, .. } | Node::Dir { mode, .. } => *mode,
        }
    }

    fn metadata(&self) -> FsMetadata {
        FsMetadata {
            kind: self.kind(),
            len: match self {
                Node::File { bytes, .. } => bytes.len() as u64,
                // A real directory reports its allocated size; meaningless here, so zero.
                Node::Dir { .. } => 0,
            },
            mode: Some(self.mode() & 0o7777),
            modified: Some(match self {
                Node::File { modified, .. } | Node::Dir { modified, .. } => *modified,
            }),
        }
    }
}

#[derive(Debug)]
struct Tree {
    nodes: BTreeMap<PathBuf, Node>,
    /// A write failure waiting for the next bytes that reach the tree.
    fault: Option<io::ErrorKind>,
}

impl Default for Tree {
    fn default() -> Self {
        // The root always exists, as on a real filesystem.
        let mut nodes = BTreeMap::new();
        nodes.insert(PathBuf::from("/"), Node::fresh_dir());
        Self { nodes, fault: None }
    }
}

impl Node {
    fn file(bytes: Vec<u8>, mode: u32) -> Self {
        Node::File {
            bytes,
            mode,
            modified: SystemTime::now(),
        }
    }

    fn fresh_dir() -> Self {
        Node::Dir {
            mode: DEFAULT_DIR_MODE,
            modified: SystemTime::now(),
        }
    }
}

/// [`Fs`] over an in-memory tree: files, directories and modes, with the same atomic-write
/// guarantees as [`crate::DiskFs`], and fault injection for the failure paths a real disk only
/// shows when it is full.
///
/// Paths are resolved lexically: without symlinks there is nothing else `..` could mean, and a
/// missing path is [`io::ErrorKind::NotFound`] wherever the real disk would say `ENOENT`. Error
/// kinds otherwise mirror what the same call returns on Linux (`IsADirectory` for reading a
/// directory, `NotADirectory` for a file in place of a directory, and so on), so a test that
/// asserts on a kind holds against either adapter.
#[derive(Debug, Default)]
pub struct MemFs {
    tree: Mutex<Tree>,
}

impl MemFs {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Make the next write that reaches the tree fail with `kind` part-way through — after some
    /// bytes of the new contents are written, as `ENOSPC` does.
    ///
    /// The failure consumes the fault: one armed fault fails exactly one write. A faulted write
    /// leaves the tree unchanged — the old contents, if any, stay whole, and no swap file appears
    /// — however the bytes would have landed. Arming a second fault replaces the first.
    pub fn fail_next_write(&self, kind: io::ErrorKind) {
        self.lock().fault = Some(kind);
    }

    /// Every path the tree holds, files and directories, sorted.
    #[must_use]
    pub fn paths(&self) -> Vec<PathBuf> {
        self.lock().nodes.keys().cloned().collect()
    }

    fn lock(&self) -> MutexGuard<'_, Tree> {
        // A poisoned lock means an earlier caller panicked mid-operation; the tree it guards is
        // whole (every mutation completes before the lock is released), so there is no torn
        // state to inherit.
        self.tree
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Resolve `path` to the key the tree holds it under: `.` dropped, `..` applied, clamped at the
/// root. The empty path names nothing, so it is [`io::ErrorKind::NotFound`] everywhere.
fn normalized(path: &Path) -> io::Result<PathBuf> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            Component::RootDir => out.push(Component::RootDir),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    // Nothing left to pop: above the start of a relative path, where `..` stays.
                    // (`pop` on a root leaves the root, so an absolute path clamps there.)
                    out.push("..");
                }
            }
            Component::Normal(part) => out.push(part),
        }
    }
    if out.as_os_str().is_empty() {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{}: no such file or directory", path.display()),
        ))
    } else {
        Ok(out)
    }
}

/// Fail the write when a fault is armed, consuming it.
fn take_fault(tree: &mut Tree, path: &Path) -> io::Result<()> {
    if let Some(kind) = tree.fault.take() {
        return Err(io::Error::new(
            kind,
            format!("{}: injected write failure", path.display()),
        ));
    }
    Ok(())
}

/// Create `dir` and every ancestor it needs, as [`std::fs::create_dir_all`]: a missing level is
/// created, a file in the way of an intermediate level is `NotADirectory`, and the final level
/// itself being a file is `AlreadyExists`.
fn ensure_dir(tree: &mut Tree, dir: &Path) -> io::Result<()> {
    let levels: Vec<PathBuf> = dir.ancestors().map(Path::to_path_buf).collect();
    for level in levels.iter().rev() {
        if level.as_os_str().is_empty() {
            continue;
        }
        match tree.nodes.get(level) {
            None => {
                tree.nodes.insert(level.clone(), Node::fresh_dir());
            }
            Some(Node::Dir { .. }) => {}
            Some(Node::File { .. }) => {
                let kind = if level == dir {
                    io::ErrorKind::AlreadyExists
                } else {
                    io::ErrorKind::NotADirectory
                };
                return Err(io::Error::new(
                    kind,
                    format!("{}: not a directory", level.display()),
                ));
            }
        }
    }
    Ok(())
}

/// The parent chain `path` needs must already exist as directories: a missing level is `NotFound`,
/// a file in the way is `NotADirectory`. The empty parent of a bare filename needs nothing.
fn require_parent_dirs(tree: &Tree, path: &Path) -> io::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    if parent.as_os_str().is_empty() {
        return Ok(());
    }
    for level in parent.ancestors() {
        if level.as_os_str().is_empty() {
            continue;
        }
        match tree.nodes.get(level) {
            Some(Node::Dir { .. }) => {}
            Some(Node::File { .. }) => {
                return Err(io::Error::new(
                    io::ErrorKind::NotADirectory,
                    format!("{}: not a directory", level.display()),
                ));
            }
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{}: no such file or directory", level.display()),
                ));
            }
        }
    }
    Ok(())
}

fn not_found(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{}: no such file or directory", path.display()),
    )
}

fn is_a_directory(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::IsADirectory,
        format!("{}: is a directory", path.display()),
    )
}

fn not_a_directory(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotADirectory,
        format!("{}: not a directory", path.display()),
    )
}

impl Fs for MemFs {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let key = normalized(path)?;
        match self.lock().nodes.get(&key) {
            Some(Node::File { bytes, .. }) => Ok(bytes.clone()),
            Some(Node::Dir { .. }) => Err(is_a_directory(path)),
            None => Err(not_found(path)),
        }
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        let bytes = self.read(path)?;
        String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn write(&self, path: &Path, contents: &[u8]) -> io::Result<()> {
        let key = normalized(path)?;
        let mut tree = self.lock();
        require_parent_dirs(&tree, &key)?;
        if matches!(tree.nodes.get(&key), Some(Node::Dir { .. })) {
            return Err(is_a_directory(path));
        }
        take_fault(&mut tree, path)?;
        // A truncated file keeps its mode; a new one lands at the default.
        let mode = match tree.nodes.get(&key) {
            Some(Node::File { mode, .. }) => *mode,
            _ => DEFAULT_FILE_MODE,
        };
        tree.nodes.insert(key, Node::file(contents.to_vec(), mode));
        Ok(())
    }

    fn append(&self, path: &Path, contents: &[u8]) -> io::Result<()> {
        let key = normalized(path)?;
        let mut tree = self.lock();
        require_parent_dirs(&tree, &key)?;
        if matches!(tree.nodes.get(&key), Some(Node::Dir { .. })) {
            return Err(is_a_directory(path));
        }
        take_fault(&mut tree, path)?;
        match tree.nodes.get_mut(&key) {
            Some(Node::File {
                bytes, modified, ..
            }) => {
                bytes.extend_from_slice(contents);
                *modified = SystemTime::now();
            }
            Some(Node::Dir { .. }) => unreachable!("directory checked above"),
            None => {
                tree.nodes
                    .insert(key, Node::file(contents.to_vec(), DEFAULT_FILE_MODE));
            }
        }
        Ok(())
    }

    fn write_atomic(&self, path: &Path, contents: &[u8], mode: Option<u32>) -> io::Result<()> {
        let key = normalized(path)?;
        let mut tree = self.lock();
        if let Some(parent) = key.parent() {
            if !parent.as_os_str().is_empty() {
                ensure_dir(&mut tree, parent)?;
            }
        }
        if matches!(tree.nodes.get(&key), Some(Node::Dir { .. })) {
            // The disk fails here too: `rename` refuses to replace a directory with a file.
            return Err(is_a_directory(path));
        }
        take_fault(&mut tree, path)?;
        // The swap file never enters the tree: it is built aside and committed whole, so a
        // failure — here or anywhere above — leaves the old contents and no swap behind.
        let mode = match (mode, tree.nodes.get(&key)) {
            (Some(mode), _) => mode,
            (None, Some(Node::File { mode, .. })) => *mode,
            (None, _) => DEFAULT_FILE_MODE,
        };
        tree.nodes.insert(key, Node::file(contents.to_vec(), mode));
        Ok(())
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        let key = normalized(path)?;
        ensure_dir(&mut self.lock(), &key)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        let key = normalized(path)?;
        let mut tree = self.lock();
        match tree.nodes.get(&key) {
            Some(Node::Dir { .. }) => Err(is_a_directory(path)),
            Some(Node::File { .. }) => {
                tree.nodes.remove(&key);
                Ok(())
            }
            None => Err(not_found(path)),
        }
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        let key = normalized(path)?;
        let mut tree = self.lock();
        match tree.nodes.get(&key) {
            None => Err(not_found(path)),
            Some(Node::File { .. }) => Err(not_a_directory(path)),
            Some(Node::Dir { .. }) => {
                if key == Path::new("/") {
                    // The root itself is never removed; its whole subtree is.
                    tree.nodes.retain(|p, _| *p == key);
                } else {
                    tree.nodes.retain(|p, _| *p != key && !p.starts_with(&key));
                }
                Ok(())
            }
        }
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let old = normalized(from)?;
        let new = normalized(to)?;
        if old == Path::new("/") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot rename the filesystem root",
            ));
        }
        let mut tree = self.lock();
        let old_node = tree.nodes.get(&old).ok_or_else(|| not_found(from))?.clone();
        if old == new {
            return Ok(());
        }
        if new.starts_with(&old) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{}: cannot move a directory into itself", to.display()),
            ));
        }
        require_parent_dirs(&tree, &new)?;
        match tree.nodes.get(&new) {
            Some(Node::Dir { .. }) if matches!(old_node, Node::File { .. }) => {
                return Err(is_a_directory(to));
            }
            Some(Node::File { .. }) if matches!(old_node, Node::Dir { .. }) => {
                return Err(not_a_directory(to));
            }
            Some(Node::Dir { .. }) => {
                let empty = !tree.nodes.keys().any(|p| p != &new && p.starts_with(&new));
                if !empty {
                    return Err(io::Error::new(
                        io::ErrorKind::DirectoryNotEmpty,
                        format!("{}: directory not empty", to.display()),
                    ));
                }
                tree.nodes.remove(&new);
            }
            Some(Node::File { .. }) => {
                tree.nodes.remove(&new);
            }
            None => {}
        }
        if matches!(old_node, Node::Dir { .. }) {
            let moved: Vec<(PathBuf, Node)> = tree
                .nodes
                .iter()
                .filter(|(p, _)| *p != &old && p.starts_with(&old))
                .map(|(p, node)| {
                    let rest = p.strip_prefix(&old).expect("filtered by prefix");
                    (new.join(rest), node.clone())
                })
                .collect();
            tree.nodes.retain(|p, _| *p != old && !p.starts_with(&old));
            tree.nodes.extend(moved);
        } else {
            tree.nodes.remove(&old);
        }
        tree.nodes.insert(new, old_node);
        Ok(())
    }

    fn metadata(&self, path: &Path) -> io::Result<FsMetadata> {
        let key = normalized(path)?;
        self.lock()
            .nodes
            .get(&key)
            .map(Node::metadata)
            .ok_or_else(|| not_found(path))
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let key = normalized(path)?;
        let tree = self.lock();
        match tree.nodes.get(&key) {
            None => Err(not_found(path)),
            Some(Node::File { .. }) => Err(not_a_directory(path)),
            Some(Node::Dir { .. }) => Ok(tree
                .nodes
                .keys()
                .filter(|p| p.parent() == Some(key.as_path()))
                .cloned()
                .collect()),
        }
    }

    fn set_mode(&self, path: &Path, mode: u32) -> io::Result<()> {
        let key = normalized(path)?;
        let mut tree = self.lock();
        match tree.nodes.get_mut(&key) {
            Some(Node::File { mode: slot, .. } | Node::Dir { mode: slot, .. }) => {
                *slot = mode;
                Ok(())
            }
            None => Err(not_found(path)),
        }
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        let key = normalized(path)?;
        if self.lock().nodes.contains_key(&key) {
            Ok(key)
        } else {
            Err(not_found(path))
        }
    }
}

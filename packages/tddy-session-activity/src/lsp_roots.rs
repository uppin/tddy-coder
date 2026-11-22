//! The code roots an orchestrator added for its LSP queries (`#orchestra` 8/9), persisted in its
//! session directory so they outlive a resume and die with the session.
//!
//! A root in the orchestrator's reach is added at once; one outside it is added once the operator
//! allows it (the `RequestPathAccess` flow). Removing is only ever for a root this session added,
//! and needs no request: it leaves this list, and the index daemon unloads the root's server once
//! no session uses it (its idle reaping), so a root another session still queries is never stopped
//! from here.

use std::path::{Path, PathBuf};

/// The file, in a session's directory, that holds the roots it added.
pub const LSP_ROOTS_FILE: &str = "lsp-roots.json";

/// One root a session added.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AddedLspRoot {
    /// An absolute path.
    pub root: PathBuf,
    pub reason: String,
    pub added_at_unix_ms: u64,
}

/// The roots one session added.
#[derive(Debug, Clone)]
pub struct SessionLspRoots {
    session_dir: PathBuf,
}

impl SessionLspRoots {
    /// The roots of the session whose directory is `session_dir`. Opening never touches disk.
    pub fn open(session_dir: &Path) -> Self {
        Self {
            session_dir: session_dir.to_path_buf(),
        }
    }

    /// Every root the session added, oldest first; none when the file does not exist.
    pub fn roots(&self) -> std::io::Result<Vec<AddedLspRoot>> {
        let bytes = match std::fs::read(self.file()) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        serde_json::from_slice(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Record `root`; adding a root the session already added keeps the first record.
    pub fn add(&self, root: AddedLspRoot) -> std::io::Result<()> {
        let mut roots = self.roots()?;
        if roots.iter().any(|added| added.root == root.root) {
            return Ok(());
        }
        roots.push(root);
        self.write(&roots)
    }

    /// Remove `root`: `true` when the session had added it, `false` — and nothing changed — when it
    /// had not.
    pub fn remove(&self, root: &Path) -> std::io::Result<bool> {
        let mut roots = self.roots()?;
        let before = roots.len();
        roots.retain(|added| added.root != root);
        if roots.len() == before {
            return Ok(false);
        }
        self.write(&roots)?;
        Ok(true)
    }

    /// Written atomically.
    fn write(&self, roots: &[AddedLspRoot]) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(roots)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::create_dir_all(&self.session_dir)?;
        let staging = self.session_dir.join(format!("{LSP_ROOTS_FILE}.tmp"));
        std::fs::write(&staging, bytes)?;
        std::fs::rename(&staging, self.file())
    }

    fn file(&self) -> PathBuf {
        self.session_dir.join(LSP_ROOTS_FILE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_root(path: &str) -> AddedLspRoot {
        AddedLspRoot {
            root: PathBuf::from(path),
            reason: "trace the API".to_string(),
            added_at_unix_ms: 1_795_000_000_000,
        }
    }

    #[test]
    fn added_roots_are_listed_oldest_first_and_survive_reopening() {
        let session = tempfile::tempdir().unwrap();
        SessionLspRoots::open(session.path())
            .add(a_root("/w/a"))
            .unwrap();
        SessionLspRoots::open(session.path())
            .add(a_root("/w/b"))
            .unwrap();

        let roots = SessionLspRoots::open(session.path()).roots().unwrap();

        assert_eq!(roots, vec![a_root("/w/a"), a_root("/w/b")]);
    }

    #[test]
    fn adding_a_root_twice_keeps_one() {
        let session = tempfile::tempdir().unwrap();
        let roots = SessionLspRoots::open(session.path());
        roots.add(a_root("/w/a")).unwrap();

        roots.add(a_root("/w/a")).unwrap();

        assert_eq!(roots.roots().unwrap().len(), 1);
    }

    #[test]
    fn removing_an_added_root_drops_it() {
        let session = tempfile::tempdir().unwrap();
        let roots = SessionLspRoots::open(session.path());
        roots.add(a_root("/w/a")).unwrap();

        let removed = roots.remove(Path::new("/w/a")).unwrap();

        assert!(removed);
        assert_eq!(roots.roots().unwrap(), Vec::new());
    }

    #[test]
    fn removing_a_root_never_added_changes_nothing() {
        let session = tempfile::tempdir().unwrap();
        let roots = SessionLspRoots::open(session.path());
        roots.add(a_root("/w/a")).unwrap();

        let removed = roots.remove(Path::new("/w/other")).unwrap();

        assert!(!removed);
        assert_eq!(roots.roots().unwrap(), vec![a_root("/w/a")]);
    }
}

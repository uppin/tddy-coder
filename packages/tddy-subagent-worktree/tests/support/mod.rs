//! A caller's worktree on disk, described by what it holds — the one fixture every suite here
//! builds on. Real repositories only: every assertion in these suites is about what git recorded,
//! and a double of git would assert nothing.

#![allow(dead_code)] // each suite uses a different slice of this module

use std::path::{Path, PathBuf};
use std::process::Command;

use tddy_subagent_worktree::{ConversationId, ConversationWorktrees};

pub const SESSION_ID: &str = "sess-1";

/// A caller worktree still being described.
pub struct CallerWorktreeBuilder {
    committed: Vec<(String, String)>,
    committed_binary: Vec<(String, Vec<u8>)>,
    staged: Vec<(String, String)>,
    unstaged: Vec<(String, String)>,
    untracked: Vec<(String, String)>,
    ignored: Vec<(String, String)>,
    linked: bool,
}

/// A caller worktree on disk: a git checkout with one initial commit, plus whatever the builder
/// added. Dropped with its temporary directory.
pub struct CallerWorktree {
    _dir: tempfile::TempDir,
    path: PathBuf,
}

pub fn a_caller_worktree() -> CallerWorktreeBuilder {
    CallerWorktreeBuilder {
        committed: vec![("README.md".into(), "hello\n".into())],
        committed_binary: vec![],
        staged: vec![],
        unstaged: vec![],
        untracked: vec![],
        ignored: vec![],
        linked: false,
    }
}

impl CallerWorktreeBuilder {
    pub fn with_committed_file(mut self, path: &str, content: &str) -> Self {
        self.committed.push((path.into(), content.into()));
        self
    }

    pub fn with_committed_binary_file(mut self, path: &str, content: &[u8]) -> Self {
        self.committed_binary.push((path.into(), content.to_vec()));
        self
    }

    /// A change the caller has `git add`ed but not committed.
    pub fn with_staged_edit(mut self, path: &str, content: &str) -> Self {
        self.staged.push((path.into(), content.into()));
        self
    }

    /// A change to a tracked file the caller has not staged.
    pub fn with_unstaged_edit(mut self, path: &str, content: &str) -> Self {
        self.unstaged.push((path.into(), content.into()));
        self
    }

    pub fn with_untracked_file(mut self, path: &str, content: &str) -> Self {
        self.untracked.push((path.into(), content.into()));
        self
    }

    /// A file `.gitignore` (committed) excludes.
    pub fn with_ignored_file(mut self, path: &str, content: &str) -> Self {
        self.ignored.push((path.into(), content.into()));
        self
    }

    /// Make the caller a *linked* worktree of another repository, the way a session worktree
    /// usually is, so its `.git` is a file pointing into a common dir elsewhere.
    pub fn in_a_linked_worktree(mut self) -> Self {
        self.linked = true;
        self
    }

    pub fn build(self) -> CallerWorktree {
        let dir = tempfile::tempdir().expect("temp dir");
        let main = dir.path().join("main");
        std::fs::create_dir_all(&main).expect("create main checkout");
        git(&main, &["init", "-q", "-b", "master"]);
        git(&main, &["config", "user.email", "developer@example.com"]);
        git(&main, &["config", "user.name", "Developer"]);
        let mut committed = self.committed.clone();
        if !self.ignored.is_empty() {
            let patterns: String = self
                .ignored
                .iter()
                .map(|(path, _)| format!("{path}\n"))
                .collect();
            committed.push((".gitignore".into(), patterns));
        }
        for (path, content) in &committed {
            write(&main, path, content.as_bytes());
        }
        for (path, content) in &self.committed_binary {
            write(&main, path, content);
        }
        git(&main, &["add", "-A"]);
        git(&main, &["commit", "-q", "-m", "initial"]);

        let path = if self.linked {
            let linked = dir.path().join("session-worktree");
            git(
                &main,
                &[
                    "worktree",
                    "add",
                    "-q",
                    "-b",
                    "session",
                    linked.to_str().expect("utf-8"),
                ],
            );
            linked
        } else {
            main
        };

        for (path_in, content) in &self.staged {
            write(&path, path_in, content.as_bytes());
            git(&path, &["add", path_in]);
        }
        for (path_in, content) in &self.unstaged {
            write(&path, path_in, content.as_bytes());
        }
        for (path_in, content) in self.untracked.iter().chain(&self.ignored) {
            write(&path, path_in, content.as_bytes());
        }
        CallerWorktree { _dir: dir, path }
    }
}

impl CallerWorktree {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn conversations(&self) -> ConversationWorktrees {
        ConversationWorktrees::new(&self.path, SESSION_ID)
    }

    pub fn head(&self) -> String {
        rev_parse(&self.path, "HEAD")
    }

    pub fn current_branch(&self) -> String {
        git(&self.path, &["branch", "--show-current"])
            .trim()
            .to_string()
    }

    /// `git status --porcelain --untracked-files=all`, one entry per line.
    pub fn status(&self) -> Vec<String> {
        git(
            &self.path,
            &["status", "--porcelain", "--untracked-files=all"],
        )
        .lines()
        .map(str::to_string)
        .collect()
    }

    /// The staged tree, so a test can prove the index was left alone.
    pub fn index_tree(&self) -> String {
        git(&self.path, &["write-tree"]).trim().to_string()
    }

    pub fn branches(&self) -> Vec<String> {
        git(
            &self.path,
            &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
        )
        .lines()
        .map(str::to_string)
        .collect()
    }

    pub fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.path.join(path)).expect("read caller file")
    }

    pub fn write(&self, path: &str, content: &str) {
        write(&self.path, path, content.as_bytes());
    }

    pub fn exists(&self, path: &str) -> bool {
        self.path.join(path).exists()
    }
}

pub fn conversation(id: &str) -> ConversationId {
    ConversationId::parse(id).expect("a safe conversation id")
}

/// Write a file inside a conversation worktree (or any checkout), creating parent directories.
pub fn write(root: &Path, path: &str, content: &[u8]) {
    let full = root.join(path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).expect("create parent dirs");
    }
    std::fs::write(full, content).expect("write file");
}

pub fn remove(root: &Path, path: &str) {
    std::fs::remove_file(root.join(path)).expect("remove file");
}

/// `git rev-parse --short HEAD` — the abbreviation git itself chooses, which is what a summary
/// must carry.
pub fn short_head(root: &Path) -> String {
    git(root, &["rev-parse", "--short", "HEAD"])
        .trim()
        .to_string()
}

pub fn rev_parse(root: &Path, rev: &str) -> String {
    git(root, &["rev-parse", rev]).trim().to_string()
}

/// The files a commit holds, as `path: content` for every text file.
pub fn file_at(root: &Path, rev: &str, path: &str) -> String {
    git(root, &["show", &format!("{rev}:{path}")])
}

/// Subjects of the commits on `branch` after `base`, oldest first.
pub fn subjects_after(root: &Path, base: &str, branch: &str) -> Vec<String> {
    git(
        root,
        &[
            "log",
            "--reverse",
            "--format=%s",
            &format!("{base}..{branch}"),
        ],
    )
    .lines()
    .map(str::to_string)
    .collect()
}

pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?} in {} failed: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8 git output")
}

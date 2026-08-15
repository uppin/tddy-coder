//! The managed destination — a directory the syncer owns outright.
//!
//! Ownership is the whole safety story. A directory the syncer adopts silently is a directory whose
//! contents it will overwrite without anyone having agreed to that, so an unmarked non-empty
//! destination and one marked for another session are both refused rather than resolved.
//!
//! Product contract: `docs/ft/daemon/session-worktree-sync.md` § Client — the managed mirror.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde::{Deserialize, Serialize};

use crate::apply::{ApplyOutcome, Delta, ReconcileReason};

/// The file that says "the syncer owns this directory, and for which session".
///
/// Written inside the destination rather than beside it: a marker one directory up would be lost by
/// a move and would make two mirrors under one parent indistinguishable.
pub const MARKER_FILENAME: &str = ".tddy-session-sync.json";

/// What the marker records, which is exactly what the syncer needs to resume without re-deriving
/// anything from the room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirrorMarker {
    pub session_id: String,
    pub daemon_instance_id: String,
    pub project: String,
    /// The last tick whose delta was applied. `0` before any has been.
    #[serde(default)]
    pub last_seq: u64,
    /// The commit the mirror was last reset to.
    #[serde(default)]
    pub last_head_commit: String,
}

/// Why a destination could not be opened, or an operation on it could not complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirrorError {
    /// The path exists, is non-empty, and carries no marker. Refused rather than adopted.
    NotOwned {
        path: PathBuf,
    },
    /// The path carries a marker for a different session. Refused rather than re-pointed.
    ForeignSession {
        path: PathBuf,
        expected_session_id: String,
        found_session_id: String,
    },
    /// A git command failed. Carries the command and git's own stderr, unabridged — a git error a
    /// mirror swallowed is the failure mode this whole feature exists to avoid.
    Git {
        command: String,
        stderr: String,
    },
    Io {
        path: PathBuf,
        reason: String,
    },
}

impl std::fmt::Display for MirrorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MirrorError::NotOwned { path } => write!(
                f,
                "{} is not empty and carries no {MARKER_FILENAME}; \
                 refusing to overwrite a directory this syncer does not own",
                path.display()
            ),
            MirrorError::ForeignSession {
                path,
                expected_session_id,
                found_session_id,
            } => write!(
                f,
                "{} mirrors session {found_session_id}, not {expected_session_id}",
                path.display()
            ),
            MirrorError::Git { command, stderr } => write!(f, "git {command} failed: {stderr}"),
            MirrorError::Io { path, reason } => write!(f, "{}: {reason}", path.display()),
        }
    }
}

impl std::error::Error for MirrorError {}

/// A destination the syncer owns.
#[derive(Debug)]
pub struct Mirror {
    dest: PathBuf,
    marker: MirrorMarker,
}

impl Mirror {
    /// Open the mirror at `dest`, or take ownership of it when it is absent or empty.
    ///
    /// Refuses rather than adopts: see [`MirrorError::NotOwned`] and
    /// [`MirrorError::ForeignSession`].
    ///
    /// A destination that already carries our marker resumes from the **persisted** one rather
    /// than from `marker`: the caller builds `marker` from what the room says now, which knows
    /// nothing of the deltas an earlier run already applied, so resuming from it would replay the
    /// session from its first tick.
    pub fn open_or_create(dest: &Path, marker: MirrorMarker) -> Result<Self, MirrorError> {
        if let Some(persisted) = read_marker(dest)? {
            if persisted.session_id != marker.session_id {
                return Err(MirrorError::ForeignSession {
                    path: dest.to_path_buf(),
                    expected_session_id: marker.session_id,
                    found_session_id: persisted.session_id,
                });
            }
            return Ok(Self {
                dest: dest.to_path_buf(),
                marker: persisted,
            });
        }

        if !is_adoptable(dest)? {
            return Err(MirrorError::NotOwned {
                path: dest.to_path_buf(),
            });
        }

        std::fs::create_dir_all(dest).map_err(|e| MirrorError::Io {
            path: dest.to_path_buf(),
            reason: e.to_string(),
        })?;
        let mirror = Self {
            dest: dest.to_path_buf(),
            marker,
        };
        mirror.write_marker()?;
        Ok(mirror)
    }

    /// The marker as it currently stands on disk.
    pub fn marker(&self) -> &MirrorMarker {
        &self.marker
    }

    /// The mirror's current `HEAD` sha.
    pub fn head_commit(&self) -> Result<String, MirrorError> {
        let output = self.git(&["rev-parse", "HEAD"], &[])?;
        if !output.status.success() {
            return Err(git_failed("rev-parse HEAD", &output));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Read a file from the mirror, as bytes.
    pub fn read(&self, rel_path: &str) -> Result<Vec<u8>, MirrorError> {
        let path = self.dest.join(rel_path);
        std::fs::read(&path).map_err(|e| MirrorError::Io {
            path,
            reason: e.to_string(),
        })
    }

    /// Apply one delta, or say why it cannot be applied.
    ///
    /// Never partially applies: a patch that does not apply cleanly leaves the mirror exactly as it
    /// was, so the reconcile that follows starts from a state the syncer can describe.
    pub fn apply(&mut self, delta: &Delta) -> Result<ApplyOutcome, MirrorError> {
        // Several tool calls landing in one poll window each name the same tick, so the same
        // delta arrives once per call. De-duplicating by `seq` is what makes it apply once.
        if delta.seq <= self.marker.last_seq {
            return Ok(ApplyOutcome::AlreadyApplied);
        }
        if delta.prev_seq != self.marker.last_seq {
            return Ok(ApplyOutcome::NeedsReconcile(ReconcileReason::SequenceGap {
                expected: self.marker.last_seq + 1,
                found: delta.seq,
            }));
        }

        let head = self.head_commit()?;
        if delta.base_commit != head {
            return Ok(ApplyOutcome::NeedsReconcile(
                ReconcileReason::BaseCommitMismatch {
                    expected: head,
                    found: delta.base_commit.clone(),
                },
            ));
        }

        // `--check` first. It separates "this patch cannot apply here" — a divergence to
        // reconcile — from "git failed", and it settles that without git having opened a single
        // file for writing. `git apply` is atomic per invocation, but that is a promise about a
        // run which completes; one killed mid-write is not covered, and a half-patched mirror is
        // exactly the state no reconcile can describe.
        let checked = self.git(&["apply", "--check"], &delta.patch)?;
        if !checked.status.success() {
            return Ok(ApplyOutcome::NeedsReconcile(
                ReconcileReason::PatchRejected {
                    detail: stderr_of(&checked),
                },
            ));
        }

        let applied = self.git(&["apply"], &delta.patch)?;
        if !applied.status.success() {
            // `--check` passed and the apply still failed, so this is not a divergence the
            // mirror can reconcile away — it is git failing at something it just said it could.
            return Err(git_failed("apply", &applied));
        }

        self.marker.last_seq = delta.seq;
        self.write_marker()?;
        Ok(ApplyOutcome::Applied)
    }

    /// Record that the mirror was restored from git — a fetch of the WIP ref and a reset onto it —
    /// rather than advanced by a patch.
    ///
    /// `applied_seq` is the newest tick the restore is **known** to include, and the caller passes
    /// what it can prove rather than what it suspects: a sequence recorded higher than the mirror's
    /// actual state makes the next delta look already-applied, and skipping a delta is the one
    /// divergence nothing downstream reports. Recording it lower costs one further reconcile, which
    /// is logged.
    pub fn record_restored(&mut self, applied_seq: u64) -> Result<(), MirrorError> {
        self.marker.last_head_commit = self.head_commit()?;
        self.marker.last_seq = applied_seq;
        self.write_marker()
    }

    /// Persist the marker, which is what lets a restart resume rather than replay.
    fn write_marker(&self) -> Result<(), MirrorError> {
        let path = self.dest.join(MARKER_FILENAME);
        let body = serde_json::to_vec_pretty(&self.marker).map_err(|e| MirrorError::Io {
            path: path.clone(),
            reason: e.to_string(),
        })?;
        std::fs::write(&path, body).map_err(|e| MirrorError::Io {
            path,
            reason: e.to_string(),
        })
    }

    /// Run git in the mirror, feeding `stdin` to it. Returns the [`Output`] rather than a
    /// `Result` of the stdout, because a non-zero git is a refusal to some callers and an error to
    /// others — `git apply` failing is a reconcile, `git rev-parse` failing is not.
    fn git(&self, args: &[&str], stdin: &[u8]) -> Result<Output, MirrorError> {
        let mut child = Command::new("git")
            .args(args)
            .current_dir(&self.dest)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| MirrorError::Io {
                path: self.dest.clone(),
                reason: format!("failed to run git {}: {e}", args.join(" ")),
            })?;
        child
            .stdin
            .take()
            .expect("stdin was piped")
            .write_all(stdin)
            .map_err(|e| MirrorError::Io {
                path: self.dest.clone(),
                reason: format!("failed to write to git {}: {e}", args.join(" ")),
            })?;
        child.wait_with_output().map_err(|e| MirrorError::Io {
            path: self.dest.clone(),
            reason: format!("failed to wait for git {}: {e}", args.join(" ")),
        })
    }
}

/// The marker in `dest`, or `None` when there is none.
///
/// A marker that exists but does not parse is an error rather than a `None`: treating it as absent
/// would make an adoptable-looking directory out of one the syncer already owns.
fn read_marker(dest: &Path) -> Result<Option<MirrorMarker>, MirrorError> {
    let path = dest.join(MARKER_FILENAME);
    let body = match std::fs::read(&path) {
        Ok(body) => body,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(MirrorError::Io {
                path,
                reason: e.to_string(),
            })
        }
    };
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|e| MirrorError::Io {
            path,
            reason: e.to_string(),
        })
}

/// Whether an unmarked `dest` may be taken over — absent, or present and empty.
fn is_adoptable(dest: &Path) -> Result<bool, MirrorError> {
    let mut entries = match std::fs::read_dir(dest) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(e) => {
            return Err(MirrorError::Io {
                path: dest.to_path_buf(),
                reason: e.to_string(),
            })
        }
    };
    match entries.next() {
        None => Ok(true),
        Some(Ok(_)) => Ok(false),
        Some(Err(e)) => Err(MirrorError::Io {
            path: dest.to_path_buf(),
            reason: e.to_string(),
        }),
    }
}

fn git_failed(command: &str, output: &Output) -> MirrorError {
    MirrorError::Git {
        command: command.to_string(),
        stderr: stderr_of(output),
    }
}

/// Git's own message, verbatim but for the trailing newline it ends every message with — this is
/// what gets logged and what a reconcile is explained by, so nothing else is edited out of it.
fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr)
        .trim_end()
        .to_string()
}

//! What a close would remove and what that would lose — the preview, and the plan the close task
//! acts on (docs/ft/daemon/session-liveness.md § Closing a session).
//!
//! Every worktree the session owns, in the order a close removes them: its clone-placed
//! conversations' worktrees (they sit inside its own), its own worktree — on this host or on its
//! target — and a split session's checkout on its codebase daemon. Each is read for what removing
//! it would lose, or carries why it could not be read.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tddy_core::SessionMetadata;
use tddy_rpc::{Request, Status};
use tddy_service::proto::session::{
    CloseSessionCommit, CloseSessionDirtyFile, CloseSessionPreview, CloseSessionTab,
    CloseSessionWorktree, CloseSessionWorktreeOwner, PreviewCloseSessionRequest,
};
use tddy_session_activity::session_deletion::{
    is_daemon_managed_worktree, worktree_removal_applies_to,
};
use tddy_subagent_worktree::{
    ConversationId, ConversationWorktree, ConversationWorktrees, SUBAGENT_WORKTREES_DIR,
};
use tddy_worktree_service::worktree_loss::{
    read_local_worktree_loss, read_target_worktree_loss, WorktreeLoss,
};
use tddy_worktree_service::worktree_sync::DirtyChange;

use super::super::DaemonSessionHost;
use super::CloseContext;

/// One worktree a close removes: what the operator is shown, and how it is removed.
pub(super) struct PlannedWorktree {
    pub view: CloseSessionWorktree,
    pub removal: Removal,
}

/// How a planned worktree is removed.
pub(super) enum Removal {
    /// The session's own worktree on this host, forced out of its project's clone.
    Local { path: PathBuf },
    /// The session's own worktree on its target.
    Target { worktree: String },
    /// A clone-placed conversation's worktree, with its branch and base ref.
    Conversation(ConversationWorktree),
    /// A conversation's worktree whose base ref is gone: the worktree alone, out of the session's
    /// repository.
    ConversationWithoutBase {
        session_worktree: PathBuf,
        root: PathBuf,
    },
    /// The checkout a split session's codebase daemon holds: closed there, as the paired workspace
    /// session, confirming the preview that daemon gave.
    Codebase {
        daemon_instance_id: String,
        session_id: String,
        fingerprint: String,
        here: bool,
    },
    /// Removed by another worktree's step: one forwarded close removes everything the codebase
    /// daemon listed.
    WithAnother,
}

/// What a close of one session would remove and end.
pub(super) struct ClosePlan {
    /// In removal order.
    pub worktrees: Vec<PlannedWorktree>,
    pub tabs: Vec<CloseSessionTab>,
    pub fingerprint: String,
}

impl ClosePlan {
    /// Whether any worktree could not be read.
    pub fn has_an_unreadable_worktree(&self) -> bool {
        self.worktrees
            .iter()
            .any(|worktree| !worktree.view.unavailable_reason.is_empty())
    }

    /// Whether removing any worktree would lose something.
    pub fn loses_something(&self) -> bool {
        self.worktrees.iter().any(|worktree| loses(&worktree.view))
    }

    pub fn preview(&self) -> CloseSessionPreview {
        CloseSessionPreview {
            worktrees: self
                .worktrees
                .iter()
                .map(|worktree| worktree.view.clone())
                .collect(),
            tabs: self.tabs.clone(),
            fingerprint: self.fingerprint.clone(),
            needs_confirmation: self.has_an_unreadable_worktree() || self.loses_something(),
        }
    }
}

fn loses(worktree: &CloseSessionWorktree) -> bool {
    !worktree.dirty_files.is_empty() || worktree.unpushed_commit_count > 0
}

impl DaemonSessionHost {
    /// Read everything a close of the session in `ctx` would remove, and what removing it would
    /// lose.
    pub(super) async fn plan_close(&self, ctx: &CloseContext) -> ClosePlan {
        let metadata = &ctx.metadata;
        let mut worktrees = Vec::new();
        if let Some(own) = self.own_worktree_of(ctx).await {
            // Conversation worktrees are cut in a worktree on this host only: a conversation of a
            // remote agent works in that agent's clone, which the clone teardown removes.
            if let Removal::Local { path } = &own.removal {
                worktrees.extend(conversation_worktrees_in(path, &ctx.session_id).await);
            }
            worktrees.push(own);
        }
        if let Some((daemon_instance_id, codebase_session_id)) =
            tddy_session_agents::peer_session_answer::split_pairing(metadata)
        {
            worktrees.extend(
                self.codebase_worktrees(ctx, daemon_instance_id, codebase_session_id)
                    .await,
            );
        }
        let fingerprint = fingerprint_of(worktrees.iter().map(|worktree| &worktree.view));
        ClosePlan {
            tabs: self.tabs_a_close_ends(ctx).await,
            worktrees,
            fingerprint,
        }
    }

    /// The session's own worktree, when its type owns one and it records one: on its target, or
    /// on this host under a project's `.worktrees`. A worktree already gone is listed losing nothing
    /// — its removal clears what git still registers of it.
    async fn own_worktree_of(&self, ctx: &CloseContext) -> Option<PlannedWorktree> {
        let metadata = &ctx.metadata;
        let session_type = metadata.session_type.as_deref().unwrap_or_default();
        if !worktree_removal_applies_to(Some(self.coding_agents.agents()), session_type) {
            return None;
        }
        let recorded = metadata.repo_path.clone()?;
        let base = recorded_base(&ctx.session_dir);
        if let Some(target) = &metadata.target {
            let host = host_of_target(target);
            let view = self
                .read_target_worktree(ctx, &recorded, base.as_deref())
                .await;
            return Some(PlannedWorktree {
                view: CloseSessionWorktree { host, ..view },
                removal: Removal::Target { worktree: recorded },
            });
        }
        let path = PathBuf::from(&recorded);
        if !is_daemon_managed_worktree(&path) {
            return None;
        }
        let view = if path.exists() {
            read_local_worktree(&path, base.as_deref()).await
        } else {
            CloseSessionWorktree {
                branch: recorded_branch(&ctx.session_dir),
                ..CloseSessionWorktree::default()
            }
        };
        Some(PlannedWorktree {
            view: CloseSessionWorktree {
                path: recorded,
                owner: CloseSessionWorktreeOwner::Session as i32,
                ..view
            },
            removal: Removal::Local { path },
        })
    }

    /// The session's worktree on its target, read through the shell its owner reaches it with.
    async fn read_target_worktree(
        &self,
        ctx: &CloseContext,
        worktree: &str,
        base: Option<&str>,
    ) -> CloseSessionWorktree {
        let view = CloseSessionWorktree {
            path: worktree.to_string(),
            owner: CloseSessionWorktreeOwner::Session as i32,
            ..CloseSessionWorktree::default()
        };
        let shell = match self.target_shell_of(ctx) {
            Ok(shell) => shell,
            Err(reason) => return unavailable(view, reason),
        };
        let exists = shell
            .exec(tddy_remote_shell::ExecRequest::new(format!(
                "test -e {}",
                tddy_core::shell_single_quote(worktree)
            )))
            .await;
        match exists {
            Err(error) => unavailable(view, error.to_string()),
            Ok(output) if output.status != 0 => CloseSessionWorktree {
                branch: recorded_branch(&ctx.session_dir),
                ..view
            },
            Ok(_) => match read_target_worktree_loss(shell.as_ref(), worktree, base).await {
                Ok(loss) => CloseSessionWorktree {
                    branch: target_branch(shell.as_ref(), worktree).await,
                    ..with_loss(view, loss)
                },
                Err(refusal) => unavailable(view, refusal.message().to_string()),
            },
        }
    }

    /// The shell the session's target is reached through, as its caller.
    pub(super) fn target_shell_of(
        &self,
        ctx: &CloseContext,
    ) -> Result<std::sync::Arc<dyn tddy_remote_shell::RemoteShell>, String> {
        let subject = ctx
            .subject
            .as_deref()
            .ok_or_else(|| "no caller to reach the session's target as".to_string())?;
        self.launch_sessions()
            .tab_target_shell()
            .target_of(&ctx.session_dir, subject)
            .map(|target| target.shell)
            .map_err(|refusal| refusal.message().to_string())
    }

    /// The worktrees of a split session's checkout on its codebase daemon, as that daemon previews
    /// closing the paired workspace session — asked directly when the checkout is on this daemon
    /// (a jailed codebase), forwarded otherwise. A daemon that no longer has the session holds no
    /// checkout for it; one that cannot be reached is a worktree that could not be read.
    async fn codebase_worktrees(
        &self,
        ctx: &CloseContext,
        daemon_instance_id: &str,
        codebase_session_id: &str,
    ) -> Vec<PlannedWorktree> {
        let here = daemon_instance_id
            == tddy_daemon_kernel::daemon_identity::local_instance_id_for_config(&self.config);
        let request = PreviewCloseSessionRequest {
            session_token: ctx.session_token.clone(),
            session_id: codebase_session_id.to_string(),
        };
        let previewed = if here {
            Box::pin(self.preview_close_session_at_host(Request::direct(request)))
                .await
                .map(|response| response.into_inner())
        } else {
            self.forward_preview_close(daemon_instance_id, &request)
                .await
        };
        let unreadable = |reason: String| {
            vec![PlannedWorktree {
                view: CloseSessionWorktree {
                    host: daemon_instance_id.to_string(),
                    owner: CloseSessionWorktreeOwner::Codebase as i32,
                    owner_id: codebase_session_id.to_string(),
                    unavailable_reason: reason,
                    ..CloseSessionWorktree::default()
                },
                removal: Removal::Codebase {
                    daemon_instance_id: daemon_instance_id.to_string(),
                    session_id: codebase_session_id.to_string(),
                    fingerprint: String::new(),
                    here,
                },
            }]
        };
        let preview = match previewed {
            Ok(preview) => preview,
            Err(refusal)
                if tddy_session_agents::peer_session_answer::peer_has_no_such_session(&refusal) =>
            {
                log::info!(
                    "CloseSession: daemon {daemon_instance_id} no longer has workspace session \
                     {codebase_session_id} ({}); session {} has no checkout there to remove",
                    refusal.message(),
                    ctx.session_id
                );
                return Vec::new();
            }
            Err(refusal) => return unreadable(refusal.message().to_string()),
        };
        // One forwarded close removes everything the codebase daemon listed; its removal rides on
        // the first worktree, and the rest are shown alone.
        let fingerprint = preview.fingerprint;
        preview
            .worktrees
            .into_iter()
            .enumerate()
            .map(|(index, worktree)| PlannedWorktree {
                view: CloseSessionWorktree {
                    host: if here {
                        worktree.host.clone()
                    } else {
                        daemon_instance_id.to_string()
                    },
                    owner: CloseSessionWorktreeOwner::Codebase as i32,
                    owner_id: codebase_session_id.to_string(),
                    ..worktree
                },
                removal: if index == 0 {
                    Removal::Codebase {
                        daemon_instance_id: daemon_instance_id.to_string(),
                        session_id: codebase_session_id.to_string(),
                        fingerprint: fingerprint.clone(),
                        here,
                    }
                } else {
                    Removal::WithAnother
                },
            })
            .collect()
    }

    /// Forward `PreviewCloseSession` to `daemon_instance_id`, refusing — distinctly from "that
    /// daemon has no such session" — when this daemon is not in the common room right now.
    async fn forward_preview_close(
        &self,
        daemon_instance_id: &str,
        request: &PreviewCloseSessionRequest,
    ) -> Result<CloseSessionPreview, Status> {
        let slot = self
            .peer_routing
            .common_room_slot("PreviewCloseSession")
            .map_err(not_a_missing_session)?;
        if !slot.is_connected().await {
            return Err(Status::unavailable(format!(
                "cannot reach the common room to read the checkout on daemon \
                 {daemon_instance_id}; retry once the daemons can see each other"
            )));
        }
        tddy_daemon_kernel::peer_calls::forward_preview_close_session(
            slot,
            daemon_instance_id,
            request,
        )
        .await
    }

    /// The tabs a close of `ctx`'s session ends: its main while an agent of it is live or it runs
    /// in tmux (whose agent outlives this daemon), each tab beside it, and each open conversation.
    async fn tabs_a_close_ends(&self, ctx: &CloseContext) -> Vec<CloseSessionTab> {
        let mut tabs = Vec::new();
        let live = self
            .launch_sessions()
            .live_sessions()
            .await
            .contains(&ctx.session_id);
        if live || ctx.metadata.tmux_session.is_some() {
            tabs.push(CloseSessionTab {
                tab_id: MAIN_TAB.to_string(),
                label: main_label(&ctx.metadata),
            });
        }
        tabs.extend(
            self.tabs_beside_main(&ctx.session_id)
                .await
                .into_iter()
                .map(|tab| CloseSessionTab {
                    tab_id: tab.terminal_id,
                    label: tab.label,
                }),
        );
        tabs.extend(
            self.agent_conversations
                .conversations_of(&ctx.session_id)
                .await
                .into_iter()
                .map(|(conversation_id, agent_id)| CloseSessionTab {
                    tab_id: conversation_tab_id(&conversation_id),
                    label: agent_id,
                }),
        );
        tabs
    }
}

/// The main tab's id, as the terminal surface names it.
pub(super) const MAIN_TAB: &str = tddy_cli_sessions::cli_session_manager::MAIN_TERMINAL_ID;

/// A conversation's tab id: `conversation:<id>`.
pub(super) fn conversation_tab_id(conversation_id: &str) -> String {
    format!("conversation:{conversation_id}")
}

/// The main tab's label: the session's agent, and the tmux session it runs in.
fn main_label(metadata: &SessionMetadata) -> String {
    let agent = metadata
        .session_type
        .clone()
        .unwrap_or_else(|| "agent".to_string());
    match &metadata.tmux_session {
        Some(name) => format!("{agent} in tmux {name}"),
        None => agent,
    }
}

/// A local peer refusal ("no common room configured") is `FAILED_PRECONDITION`, which a caller
/// would read as "that daemon has no such session"; it is a checkout that could not be read.
fn not_a_missing_session(refusal: Status) -> Status {
    Status::unavailable(refusal.message().to_string())
}

/// Where a target is, as the operator knows it: an ssh target's `Host` alias, a Coder workspace as
/// `owner/workspace`.
pub(super) fn host_of_target(target: &tddy_core::SessionTarget) -> String {
    match target {
        tddy_core::SessionTarget::Ssh { config_host } => config_host.clone(),
        tddy_core::SessionTarget::Coder {
            owner, workspace, ..
        } => format!("{owner}/{workspace}"),
    }
}

/// The base the session's worktree was cut from, as its changeset recorded it.
fn recorded_base(session_dir: &Path) -> Option<String> {
    tddy_core::read_changeset(session_dir)
        .ok()?
        .effective_worktree_integration_base_ref
        .filter(|base| !base.trim().is_empty())
}

/// The branch the session's changeset records — what a worktree no longer on disk was on.
fn recorded_branch(session_dir: &Path) -> String {
    tddy_core::read_changeset(session_dir)
        .ok()
        .and_then(|changeset| changeset.branch)
        .unwrap_or_default()
}

/// A worktree on this host: what removing it would lose, and the branch it is on.
async fn read_local_worktree(path: &Path, base: Option<&str>) -> CloseSessionWorktree {
    let view = CloseSessionWorktree::default();
    let (worktree, base) = (path.to_path_buf(), base.map(str::to_string));
    let read = tokio::task::spawn_blocking(move || {
        read_local_worktree_loss(&worktree, base.as_deref())
            .map(|loss| (loss, local_branch(&worktree)))
    })
    .await;
    match read {
        Ok(Ok((loss, branch))) => CloseSessionWorktree {
            branch,
            ..with_loss(view, loss)
        },
        Ok(Err(error)) => unavailable(view, error.to_string()),
        Err(error) => unavailable(view, format!("the read did not finish: {error}")),
    }
}

/// The branch checked out in `worktree`; empty on a detached `HEAD`.
fn local_branch(worktree: &Path) -> String {
    std::process::Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["symbolic-ref", "--short", "-q", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default()
}

/// The branch checked out in `worktree` on a target; empty on a detached `HEAD`.
async fn target_branch(shell: &dyn tddy_remote_shell::RemoteShell, worktree: &str) -> String {
    let command = format!(
        "git -C {} symbolic-ref --short -q HEAD",
        tddy_core::shell_single_quote(worktree)
    );
    match shell
        .exec(tddy_remote_shell::ExecRequest::new(command))
        .await
    {
        Ok(output) if output.status == 0 => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => String::new(),
    }
}

/// Every clone-placed conversation worktree under the session worktree `session_worktree`, each
/// read for what removing it would lose (above the commit the conversation started from).
async fn conversation_worktrees_in(
    session_worktree: &Path,
    session_id: &str,
) -> Vec<PlannedWorktree> {
    let Ok(entries) = std::fs::read_dir(session_worktree.join(SUBAGENT_WORKTREES_DIR)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .collect();
    names.sort();
    let worktrees = ConversationWorktrees::new(session_worktree, session_id);
    let mut planned = Vec::new();
    for name in names {
        let root = session_worktree.join(SUBAGENT_WORKTREES_DIR).join(&name);
        let without_base = || Removal::ConversationWithoutBase {
            session_worktree: session_worktree.to_path_buf(),
            root: root.clone(),
        };
        // A directory that names no conversation, or whose conversation recorded no base, is still
        // read — removing the session's worktree would take whatever it holds along.
        let found = match ConversationId::parse(&name) {
            Ok(conversation) => worktrees.existing(&conversation).await,
            Err(_) => Ok(None),
        };
        let (read, removal) = match found {
            Ok(Some(found)) => (
                CloseSessionWorktree {
                    branch: found.branch().to_string(),
                    ..read_local_worktree(found.root(), Some(found.base())).await
                },
                Removal::Conversation(found),
            ),
            // Not a checkout of its own: git would read the enclosing session worktree instead,
            // which excludes this directory, so what it holds is never reported as safe to lose.
            Ok(None) if !root.join(".git").exists() => (
                unavailable(
                    CloseSessionWorktree::default(),
                    format!(
                        "{} is not a git worktree, so what it holds cannot be read; removing the \
                         session's worktree would delete it",
                        root.display()
                    ),
                ),
                without_base(),
            ),
            Ok(None) => (read_local_worktree(&root, None).await, without_base()),
            Err(error) => (
                unavailable(CloseSessionWorktree::default(), error.to_string()),
                without_base(),
            ),
        };
        planned.push(PlannedWorktree {
            view: CloseSessionWorktree {
                path: root.to_string_lossy().into_owned(),
                owner: CloseSessionWorktreeOwner::Conversation as i32,
                owner_id: name,
                ..read
            },
            removal,
        });
    }
    planned
}

/// `view` carrying what removing it would lose.
fn with_loss(view: CloseSessionWorktree, loss: WorktreeLoss) -> CloseSessionWorktree {
    CloseSessionWorktree {
        dirty_files: loss
            .dirty_files
            .into_iter()
            .map(|file| CloseSessionDirtyFile {
                path: file.path,
                change: change_name(file.change).to_string(),
            })
            .collect(),
        unpushed_commits: loss
            .unpushed
            .into_iter()
            .map(|commit| CloseSessionCommit {
                sha: commit.sha,
                subject: commit.subject,
            })
            .collect(),
        unpushed_commit_count: loss.unpushed_count,
        ..view
    }
}

/// `view`, which could not be read, saying why.
fn unavailable(view: CloseSessionWorktree, reason: String) -> CloseSessionWorktree {
    CloseSessionWorktree {
        unavailable_reason: reason,
        ..view
    }
}

/// A dirty file's change as the wire names it.
fn change_name(change: DirtyChange) -> &'static str {
    match change {
        DirtyChange::Modified => "modified",
        DirtyChange::Staged => "staged",
        DirtyChange::Untracked => "untracked",
        DirtyChange::Deleted => "deleted",
        DirtyChange::Renamed => "renamed",
    }
}

/// A SHA-256, hex, over every worktree's owner, owner id, path and host, and what removing it would
/// lose — its files (path and change) and its commits no remote holds (the listed shas and the full
/// count) — or why it could not be read. Order-independent: each worktree, and each list within it,
/// is sorted first, so the same state always digests the same.
pub(super) fn fingerprint_of<'a>(
    worktrees: impl IntoIterator<Item = &'a CloseSessionWorktree>,
) -> String {
    let mut digests: Vec<String> = worktrees
        .into_iter()
        .map(|worktree| {
            let mut files: Vec<String> = worktree
                .dirty_files
                .iter()
                .map(|file| format!("{}\u{1f}{}", file.path, file.change))
                .collect();
            files.sort();
            let mut commits: Vec<&str> = worktree
                .unpushed_commits
                .iter()
                .map(|commit| commit.sha.as_str())
                .collect();
            commits.sort_unstable();
            let losses = if worktree.unavailable_reason.is_empty() {
                format!(
                    "files\u{1e}{}\u{1d}commits\u{1e}{}\u{1e}{}",
                    files.join("\u{1e}"),
                    commits.join("\u{1e}"),
                    worktree.unpushed_commit_count
                )
            } else {
                format!("unavailable\u{1e}{}", worktree.unavailable_reason)
            };
            [
                worktree.owner.to_string(),
                worktree.owner_id.clone(),
                worktree.path.clone(),
                worktree.host.clone(),
                losses,
            ]
            .join("\u{1c}")
        })
        .collect();
    digests.sort();
    let digest = Sha256::digest(digests.join("\n").as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_worktree(path: &str, files: &[&str]) -> CloseSessionWorktree {
        CloseSessionWorktree {
            path: path.to_string(),
            owner: CloseSessionWorktreeOwner::Session as i32,
            dirty_files: files
                .iter()
                .map(|file| CloseSessionDirtyFile {
                    path: file.to_string(),
                    change: "untracked".to_string(),
                })
                .collect(),
            ..CloseSessionWorktree::default()
        }
    }

    #[test]
    fn the_fingerprint_does_not_depend_on_the_order_worktrees_and_files_are_listed_in() {
        // Given the same two worktrees, listed in two orders
        let one = a_worktree("/repo/.worktrees/one", &["a.md", "b.md"]);
        let two = a_worktree("/repo/.worktrees/two", &["c.md"]);
        let one_reversed = a_worktree("/repo/.worktrees/one", &["b.md", "a.md"]);

        // When
        let first = fingerprint_of([&one, &two]);
        let second = fingerprint_of([&two, &one_reversed]);

        // Then
        assert_eq!(first, second);
    }

    #[test]
    fn the_fingerprint_changes_when_a_file_is_added() {
        // Given
        let before = a_worktree("/repo/.worktrees/one", &["a.md"]);
        let after = a_worktree("/repo/.worktrees/one", &["a.md", "b.md"]);

        // When
        let (fingerprint_before, fingerprint_after) =
            (fingerprint_of([&before]), fingerprint_of([&after]));

        // Then
        assert_ne!(fingerprint_before, fingerprint_after);
    }

    #[test]
    fn an_unreadable_worktree_digests_its_reason() {
        // Given the same worktree, unreadable for two reasons
        let refused = CloseSessionWorktree {
            unavailable_reason: "Connection refused".to_string(),
            ..a_worktree("/repo/.worktrees/one", &[])
        };
        let locked = CloseSessionWorktree {
            unavailable_reason: "the vault is locked".to_string(),
            ..refused.clone()
        };

        // When
        let (first, second) = (fingerprint_of([&refused]), fingerprint_of([&locked]));

        // Then
        assert_ne!(first, second);
    }
}

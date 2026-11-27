//! Getting a worktree ready for a launch's per-session hooks file, on whichever host the worktree
//! is: `.tddy/hooks/` kept out of commits, and the daemon's entries released from the worktree-wide
//! file the hooks adapter used to write (`HooksAdapter::legacy_shared_file`).
//!
//! A claude-cli session's hooks are `<worktree>/.tddy/hooks/<session_id>/claude-settings.json`,
//! loaded with `--settings` and rendered again on every launch, start or resume
//! (`tddy_core::claude_hooks::session_file`). Before that, every session in a worktree shared its
//! `.claude/settings.local.json` and a resume never rewrote it, so the project-home session's
//! hooks reported to an orchestrator started in the same worktree, over the socket of a daemon that
//! had exited (reported 2026-11-21). Claude Code merges hooks from every settings source, so those
//! entries are released before the agent starts, or each hook would also fire as they say.

use std::path::Path;

use tddy_coding_agent_specs::slots::hooks::{adapter, LegacyRelease};
use tddy_coding_agent_specs::slots::RenderedSlots;
use tddy_coding_agent_specs::CodingAgentSpec;
use tddy_core::claude_hooks::session_file::SESSION_HOOKS_DIR;
use tddy_remote_shell::{ExecRequest, RemoteShell, RemoteShellError};
use tddy_rpc::Status;

use crate::cli_session_pointer::{inside_a_git_checkout, PointerHost};

/// Get `worktree`, on `host`, ready for the hooks `slots` hold, before they are written: when a
/// hooks file is under `.tddy/hooks/` that directory is git-excluded, and the adapter's legacy
/// shared file, if it names one, has the daemon's entries released.
pub(crate) async fn prepare_worktree_for_session_hooks(
    fs: &dyn tddy_fs::Fs,
    spec: &CodingAgentSpec,
    worktree: &Path,
    slots: &RenderedSlots,
    host: &PointerHost,
) -> Result<(), Status> {
    let hooks_dir = worktree.join(SESSION_HOOKS_DIR);
    if !slots
        .files
        .iter()
        .any(|(path, _)| path.starts_with(&hooks_dir))
    {
        return Ok(());
    }
    let adapter =
        adapter(&spec.agent.hooks.adapter).map_err(|e| Status::internal(e.to_string()))?;
    let legacy = adapter.legacy_shared_file(worktree);
    let release = |bytes: &[u8]| adapter.release_legacy_shared_file(bytes);
    match host {
        PointerHost::Local => prepare_locally(fs, worktree, legacy.as_deref(), release),
        PointerHost::Ssh { shell } => {
            prepare_through(shell, worktree, legacy.as_deref(), release).await
        }
        PointerHost::Remote { shell } => {
            prepare_through(shell.as_ref(), worktree, legacy.as_deref(), release).await
        }
    }
}

/// The `info/exclude` line that keeps every session's hooks out of commits.
fn exclude_pattern() -> String {
    format!("/{SESSION_HOOKS_DIR}/")
}

fn prepare_locally(
    fs: &dyn tddy_fs::Fs,
    worktree: &Path,
    legacy: Option<&Path>,
    release: impl Fn(&[u8]) -> LegacyRelease,
) -> Result<(), Status> {
    if inside_a_git_checkout(worktree)? {
        tddy_core::turn_files::exclude_from_git(worktree, &exclude_pattern())
            .map_err(|e| Status::internal(e.to_string()))?;
    }
    let Some(legacy) = legacy else {
        return Ok(());
    };
    let bytes = match fs.read(legacy) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(Status::internal(format!(
                "failed to read {} to release its hooks: {e}",
                legacy.display()
            )))
        }
    };
    match release(&bytes) {
        LegacyRelease::Keep => return Ok(()),
        LegacyRelease::Rewrite(bytes) => tddy_core::atomic_file::write_atomic(legacy, bytes),
        LegacyRelease::Remove => fs.remove_file(legacy),
    }
    .map_err(|e| {
        Status::internal(format!(
            "failed to release the hooks in {}: {e}",
            legacy.display()
        ))
    })?;
    log::info!(
        target: "tddy_daemon::session_hooks",
        "released the daemon's hooks from {}",
        legacy.display()
    );
    Ok(())
}

async fn prepare_through(
    shell: &dyn RemoteShell,
    worktree: &Path,
    legacy: Option<&Path>,
    release: impl Fn(&[u8]) -> LegacyRelease,
) -> Result<(), Status> {
    // One command: exclude the hooks directory in a checkout, then print the legacy file when
    // there is one — nothing at all when there is not.
    let pattern = exclude_pattern();
    let mut command = format!(
        "if exclude=$(git -C {worktree} rev-parse --path-format=absolute --git-path info/exclude 2>/dev/null); then \
         mkdir -p \"$(dirname \"$exclude\")\"; \
         grep -qxF {pattern} \"$exclude\" 2>/dev/null || echo {pattern} >> \"$exclude\"; fi",
        worktree = quote(&worktree.to_string_lossy()),
        pattern = quote(&pattern),
    );
    if let Some(legacy) = legacy {
        let path = quote(&legacy.to_string_lossy());
        command.push_str(&format!("; if [ -f {path} ]; then cat {path}; fi"));
    }
    let bytes = shell
        .exec(ExecRequest::new(command))
        .await
        .and_then(|output| output.success_or_exited())
        .map_err(|e| remote_status(&e))?
        .stdout;
    let Some(legacy) = legacy else {
        return Ok(());
    };
    let path = legacy.to_string_lossy();
    match release(&bytes) {
        LegacyRelease::Keep => return Ok(()),
        LegacyRelease::Rewrite(bytes) => shell
            .write_file(&path, &bytes, tddy_remote_shell::FileMode::TargetDefault)
            .await
            .map_err(|e| remote_status(&e))?,
        LegacyRelease::Remove => {
            shell
                .exec(ExecRequest::new(format!("rm -f {}", quote(&path))))
                .await
                .and_then(|output| output.success_or_exited())
                .map_err(|e| remote_status(&e))?;
        }
    }
    log::info!(
        target: "tddy_daemon::session_hooks",
        "released the daemon's hooks from {path} on {}",
        shell.address()
    );
    Ok(())
}

fn quote(value: &str) -> String {
    tddy_core::claude_hooks::shell_quote(value)
}

fn remote_status(error: &RemoteShellError) -> Status {
    match error {
        RemoteShellError::TimedOut { .. } => Status::deadline_exceeded(error.to_string()),
        other => Status::internal(other.to_string()),
    }
}

#[cfg(test)]
#[path = "session_hooks_worktree_tests.rs"]
mod tests;

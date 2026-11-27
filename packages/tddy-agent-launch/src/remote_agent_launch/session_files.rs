use crate::remote_agent_launch::RemoteLaunch;
use crate::remote_agent_launch::RemoteLaunchError;
use crate::spec_launch::{plan_host_pty, HostAgent, SpecLaunchError};
use std::path::Path;
use tddy_actions::tool::TemplateParams;
use tddy_coding_agent_specs::slots::RenderedSlots;
use tddy_remote_shell::{FileMode, RemoteShell};

pub(super) async fn plan_the_launch(
    agent: &HostAgent,
    params: &TemplateParams,
    slots: &RenderedSlots,
    remote: &RemoteLaunch<'_>,
) -> Result<
    (
        Vec<(std::path::PathBuf, Vec<u8>)>,
        crate::spec_launch::HostPtyPlan,
    ),
    RemoteLaunchError,
> {
    let mut render_slots = slots.clone();
    let slot_files = std::mem::take(&mut render_slots.files);
    // The agent is the target's own, found by its bare command on the target's login `PATH`: the
    // binary this host resolved means nothing there.
    let mut on_the_target = agent.clone();
    on_the_target.tool.binary = std::path::PathBuf::from(&agent.tool.command);
    // `on_target` lets a spec render what only a start on a target needs (Cursor's `--trust`).
    let params = params.clone().with_flag("on_target", true);
    let plan =
        plan_host_pty(&on_the_target, &params, &render_slots).map_err(RemoteLaunchError::Plan)?;
    write_slot_files(remote, &slot_files).await?;
    Ok((slot_files, plan))
}

async fn write_slot_files(
    remote: &RemoteLaunch<'_>,
    slot_files: &[(std::path::PathBuf, Vec<u8>)],
) -> Result<(), RemoteLaunchError> {
    for (path, bytes) in slot_files {
        remote
            .shell
            .write_file(&path.to_string_lossy(), bytes, FileMode::TargetDefault)
            .await
            .map_err(RemoteLaunchError::Shell)?;
    }
    Ok(())
}

/// Where a launch's rendered session files are copied to: the target a shell reaches, the session's
/// worktree there, and the session's directory on this host the files are rendered into.
pub(super) struct SessionFilesPlace<'a> {
    pub shell: &'a dyn RemoteShell,
    pub worktree: &'a Path,
    pub session_dir: &'a Path,
}

impl<'a> SessionFilesPlace<'a> {
    /// The place of a main's launch: its target, its worktree and its session directory.
    pub(super) fn of(remote: &'a RemoteLaunch<'_>) -> Self {
        Self {
            shell: remote.shell.as_ref(),
            worktree: remote.worktree,
            session_dir: remote.session_dir,
        }
    }
}

/// Write every file the plan's argv names under this host's session directory
/// (`place.session_dir`) onto the target, and return the plan with those arguments rewritten to
/// the target's copies.
///
/// The session directory maps to the worktree's `.tddy/` on the target — the directory that
/// already holds the agent's pid file, the session pointer and the hook log, git-excluded and
/// owner-only — so `<session_dir>/<relative>` is written at `<worktree>/.tddy/<relative>`. The copy
/// is written owner-only: a file in the session directory can hold a secret. An argument naming a
/// slot file is left alone, since the slot files were already written at the paths the agent is
/// told. Only the argv is mapped; an environment value naming a session file is not.
pub(super) async fn write_rendered_session_files(
    fs: &dyn tddy_fs::Fs,
    place: &SessionFilesPlace<'_>,
    slot_files: Vec<(std::path::PathBuf, Vec<u8>)>,
    mut plan: crate::spec_launch::HostPtyPlan,
) -> Result<crate::spec_launch::HostPtyPlan, RemoteLaunchError> {
    let on_the_target = place.worktree.join(".tddy");
    for argument in &mut plan.argv {
        let path = Path::new(argument.as_str());
        let Ok(relative) = path.strip_prefix(place.session_dir) else {
            continue;
        };
        // `metadata_follow`, as before: `is_file` follows a link.
        if !matches!(fs.metadata_follow(path), Ok(m) if m.kind == tddy_fs::FsKind::File)
            || slot_files.iter().any(|(written, _)| written == path)
        {
            continue;
        }
        let bytes = fs.read(path).map_err(|error| {
            RemoteLaunchError::Plan(SpecLaunchError::SlotFile {
                path: path.to_path_buf(),
                message: error.to_string(),
            })
        })?;
        let target_path = on_the_target.join(relative).to_string_lossy().into_owned();
        place
            .shell
            .write_file(&target_path, &bytes, FileMode::OwnerOnly)
            .await
            .map_err(RemoteLaunchError::Shell)?;
        *argument = target_path;
    }
    Ok(plan)
}

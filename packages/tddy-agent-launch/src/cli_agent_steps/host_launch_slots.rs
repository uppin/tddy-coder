use crate::slot_files_over_shell::write_slot_files_through;
use crate::spec_launch::{AgentToolset, HostAgent, McpRegistration};
use std::path::{Path, PathBuf};
use tddy_coding_agent_specs::slots::{self, RenderedSlots, SlotInputs};
use tddy_core::HookCommandParams;
use tddy_rpc::Status;

/// What a generic host launch fills beyond its spec's own argv.
#[derive(Default)]
pub(crate) struct HostLaunchSlots<'a> {
    /// The tool server registered and the tools pre-approved and withdrawn.
    pub toolset: AgentToolset,
    /// The session its `hooks` slot writes settings for; `None` writes none (a spec without
    /// `hooks`, or a resume of a file adapter, which keeps the file its start wrote — argv
    /// adapters render again).
    pub hooks: Option<HookCommandParams<'a>>,
    /// Passed with the spec's `instructions.append_system_prompt` flag.
    pub system_prompt_file: Option<&'a Path>,
    /// `{home}` of a placed MCP path: the host's `$HOME`.
    pub home: Option<PathBuf>,
}

/// `agent`'s slots for a host launch in `worktree`, its registration files under `session_dir`. A
/// hooks file the agent does not find by itself — Claude's per-session `--settings` file, which also
/// holds a provider route's pin — is loaded by the flags its adapter renders beside it.
pub(crate) fn render_host_launch_slots(
    agent: &HostAgent,
    worktree: &Path,
    session_dir: &Path,
    slots: HostLaunchSlots<'_>,
) -> Result<RenderedSlots, Status> {
    let HostLaunchSlots {
        toolset,
        hooks,
        system_prompt_file,
        home,
    } = slots;
    let AgentToolset {
        mcp,
        allowed_tools,
        denied_tools,
    } = toolset;
    let (mcp_server, scratch_dir) = match mcp {
        Some(McpRegistration { server, dir }) => (Some(server), dir),
        None => (None, session_dir.to_path_buf()),
    };
    slots::render(
        &agent.spec,
        &SlotInputs {
            workdir: worktree.to_path_buf(),
            scratch_dir,
            mcp_server,
            allowed_tools,
            denied_tools,
            permission_prompt_tool: None,
            preamble: None,
            append_system_prompt_file: system_prompt_file.map(Path::to_path_buf),
            hooks,
            home,
        },
    )
    .map_err(|e| Status::internal(e.to_string()))
}

/// Write `slots.files` on `remote_host` over ssh and take them out of `slots`, so the launch's
/// local write (`spec_launch::plan_host_pty`) writes nothing at those paths on this host. A local
/// checkout (`None`) leaves `slots` alone. Refused `DEADLINE_EXCEEDED` naming the host when the
/// target never answers.
pub(crate) async fn write_slot_files_on_target(
    slots: &mut RenderedSlots,
    remote_host: Option<&str>,
) -> Result<(), Status> {
    let Some(host) = remote_host else {
        return Ok(());
    };
    let shell = tddy_remote_shell::SshShell::new(host);
    write_slot_files_through_shell(slots, &shell).await
}

/// [`write_slot_files_on_target`], through the shell that reaches the session's target.
pub(crate) async fn write_slot_files_through_shell(
    slots: &mut RenderedSlots,
    shell: &dyn tddy_remote_shell::RemoteShell,
) -> Result<(), Status> {
    let files = std::mem::take(&mut slots.files);
    let paths: Vec<PathBuf> = files.iter().map(|(path, _)| path.clone()).collect();
    write_slot_files_through(shell, &files).await?;
    for path in &paths {
        slots.written.retain(|written| written != path);
    }
    Ok(())
}

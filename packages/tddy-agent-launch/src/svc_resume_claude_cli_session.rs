use tddy_rpc::Status;

use crate::claude_cli_tmux::agent_tmux_session_is_alive;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;

/// The recipe a resume prepares a managed workflow for: the session's recorded one, unless its agent
/// is live in tmux and this daemon still serves its workflow.
///
/// A live tmux session holds the agent the session recorded, and its `TDDY_SOCKET` names the
/// toolcall socket of the workflow this daemon already serves: that workflow is kept, never
/// replaced — replacing it would drop the old listener, which unlinks the very socket the agent is
/// using. Only a daemon with no such workflow (a restart) prepares one, and it binds the same path.
pub(crate) async fn resumed_recipe<'m>(
    manager: &CliSessionManager,
    meta: &'m tddy_core::SessionMetadata,
    os_user: &str,
    session_id: &str,
) -> Result<Option<&'m str>, Status> {
    let Some(recipe_name) = meta.recipe.as_deref().filter(|s| !s.trim().is_empty()) else {
        return Ok(None);
    };
    let keeps_workflow = meta.tmux_session.is_some()
        && agent_tmux_session_is_alive(os_user, session_id).await?
        && manager.has_managed_workflow(session_id).await;
    Ok((!keeps_workflow).then_some(recipe_name))
}

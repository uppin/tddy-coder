use crate::launch_ports::LaunchSessions;
use crate::session_acting_identity::SessionIdentity;

use crate::svc_relaunch_sandboxed_runner::RelaunchManagedEnv;

use std::path::PathBuf;

use tddy_rpc::Status;

use std::sync::Arc;

use std::path::Path;

impl LaunchSessions {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn relaunch_managed_workflow(
        &self,
        session_id: &str,
        session_dir: &Path,
        worktree_path: &Path,
        managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe + 'static>>,
        context_dir: &Path,
        tddy_tools_path: &str,
        os_user: &str,
        identity: SessionIdentity,
    ) -> Result<RelaunchManagedEnv, Status> {
        let mut managed: Option<tddy_cli_sessions::session_toolcall::ManagedWorkflow> = None;
        let mut append_system_prompt_file: Option<PathBuf> = None;
        // As at the start: the relayed host-side commands run under the project's account
        // identity whether or not the session is managed.
        let (mut session_env, github_credential) = self.jail_relay_identity(identity, os_user)?;
        if let Some(recipe) = managed_recipe {
            let resume_goal = Self::managed_resume_goal(session_dir, &recipe);
            let launch = self.prepare_managed_workflow(
                session_id,
                recipe,
                session_dir,
                worktree_path,
                context_dir,
                tddy_tools_path,
                Some(resume_goal),
                None,
                github_credential,
            )?;
            append_system_prompt_file = Some(launch.prompt_file);
            session_env.extend(launch.env);
            managed = Some(launch.workflow);
        }
        Ok((managed, append_system_prompt_file, session_env))
    }
}

use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;

use tddy_rpc::Status;

use tddy_service::proto::exec_tools::ExecuteToolRequest;

use tddy_daemon_kernel::SessionUserResolver;

use tddy_daemon_kernel::config::DaemonConfig;

/// Authenticate an exec-tool caller, and answer with the OS user its tools run as here.
///
/// Separate from [`resolve_exec_tool_worktree`] because it has to run **before** the hosted-clone
/// branch, which resolves no worktree of this daemon's at all and whose mutating half proxies to the
/// facilitating daemon under the *clone's* stored credential. Reached with no check of its own, that
/// branch would let any common-room participant that read a session id out of a `session.agents`
/// broadcast land an arbitrary write in another host's authoritative worktree.
///
/// Both refusals name **this** daemon. For a split session the tools are served on the codebase
/// host while the error is rendered in the agent's transcript on the agent host, where an
/// unattributed "invalid or expired session" reads as the agent host's own answer — and the two
/// likeliest split misconfigurations land here: a codebase host that has not learned the agent
/// host's signing key (a session token is verifiable only by a daemon that has seen its signer's
/// public key advertised in the common room), and a GitHub user mapped on the agent host but not
/// on the codebase host. Each is also logged here, because the operator debugging it is reading
/// *this* daemon's log.
pub fn authorize_exec_tool_caller(
    config: &DaemonConfig,
    user_resolver: &SessionUserResolver,
    req: &ExecuteToolRequest,
) -> Result<String, Status> {
    let local_instance_id = local_instance_id_for_config(config);
    let Some(github_user) = (user_resolver)(&req.session_token) else {
        if let Some(expired_secs_ago) = seconds_since_expiry(&req.session_token) {
            log::warn!(
                "exec tool {tool:?} for session {session} refused on daemon {local_instance_id}: the session token expired {expired_secs_ago}s ago",
                tool = req.tool_name,
                session = req.session_id
            );
            return Err(Status::unauthenticated(format!(
                "daemon {local_instance_id} refused the session token: it expired {} ago. An agent's tddy-tools renews its token while it runs; a token that expired anyway is replaced when the daemon resumes the session, so ask the operator to resume it",
                human_duration(expired_secs_ago)
            )));
        }
        log::warn!(
            "exec tool {tool:?} for session {session} refused on daemon {local_instance_id}: the session token could not be verified here (a split session's agent presents a token its agent daemon signed with its own key, so this daemon must have seen that daemon's signing key advertised in the common room)",
            tool = req.tool_name,
            session = req.session_id
        );
        return Err(Status::unauthenticated(format!(
            "daemon {local_instance_id} could not verify the session token (invalid or expired there); a split session's tools run on the daemon holding the codebase, which verifies the token against the agent daemon's signing key as advertised in the common room"
        )));
    };
    let Some(os_user) = config.os_user_for_github(&github_user) else {
        log::warn!(
            "exec tool {tool:?} for session {session} refused on daemon {local_instance_id}: GitHub user {github_user} has no users[] entry here",
            tool = req.tool_name,
            session = req.session_id
        );
        return Err(Status::permission_denied(format!(
            "daemon {local_instance_id} has no OS user mapped for GitHub user {github_user}; add a users[] entry there — a split session's tools run as that user on the daemon holding the codebase"
        )));
    };
    Ok(os_user)
}

/// How long ago `token` expired, when it claims an expiry in the past. The claim is read without
/// verifying the token: it only picks the message, never whether the call is allowed.
fn seconds_since_expiry(token: &str) -> Option<u64> {
    let expiry = tddy_github::SessionTokenVerifier::unverified_expiry(token)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    (now > expiry).then(|| now - expiry)
}

fn human_duration(secs: u64) -> String {
    match secs {
        s if s < 120 => format!("{s}s"),
        s if s < 2 * 3600 => format!("{}m", s / 60),
        s => format!("{}h {}m", s / 3600, (s % 3600) / 60),
    }
}

#[cfg(test)]
#[path = "exec_tool_caller_tests.rs"]
mod exec_tool_caller_tests;

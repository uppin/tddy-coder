use tddy_rpc::Status;

use tddy_daemon_kernel::SessionUserResolver;

use tddy_daemon_kernel::config::DaemonConfig;

/// Resolve a caller's session token to the OS user this daemon runs its work as.
///
/// Free rather than a method so a family handler above this crate (`tddy-daemon-rpc`) authenticates
/// a caller exactly as the session host does, from the two fields it reads, without holding the
/// host.
pub fn resolve_os_user(
    config: &DaemonConfig,
    user_resolver: &SessionUserResolver,
    session_token: &str,
) -> Result<String, Status> {
    let github_user = (user_resolver)(session_token)
        .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
    config
        .os_user_for_github(&github_user)
        .map(|s| s.to_string())
        .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))
}

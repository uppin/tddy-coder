use tddy_daemon_kernel::config::DaemonConfig;

/// Where a hook command reaches this daemon when nothing is configured: its own web listener on
/// loopback.
///
/// The port default is here and nowhere else — a hook posting to the wrong port fails silently from
/// the operator's side, and three copies of `8899` is three chances for one of them to fall behind a
/// changed default.
pub fn local_daemon_hook_url(config: &DaemonConfig) -> String {
    format!(
        "http://127.0.0.1:{}",
        config.listen.web_port.unwrap_or(DEFAULT_WEB_PORT)
    )
}

/// Externally-reachable HTTP base URL peer daemons use to reach this daemon's Connect-HTTP surface
/// (today: `auth.LiveKitTokenService/MintLiveKitToken`, used by `tddy-remote-git-repo` to mint the
/// common-room LiveKit token before driving `remote_git.RemoteGitService/Serve`).
///
/// Explicit `listen.advertise_url` wins; otherwise the loopback URL derived from the web port —
/// the same default `claude_hook_daemon_url` falls back to, and for the same reason: a daemon that
/// never configured an external URL is one a peer on another host cannot reach, but one a peer on
/// the same host (and every test) can. The facilitating daemon publishes this in
/// `AgentClonePlacement.facilitating_daemon_url` so an owning daemon that has never seen the
/// project can clone it (PRD AC37).
pub fn advertise_daemon_url(config: &DaemonConfig) -> String {
    config
        .listen
        .advertise_url
        .as_deref()
        .and_then(tddy_daemon_kernel::trim_to_option)
        .unwrap_or_else(|| local_daemon_hook_url(config))
}

/// Base URL a claude-cli session's hook commands call `ReportSessionStatus` on: the configured
/// `claude_cli.daemon_url`, else this daemon's own web port.
pub fn claude_hook_daemon_url(config: &DaemonConfig) -> String {
    config
        .claude_cli
        .as_ref()
        .and_then(|c| c.daemon_url.as_deref())
        .map(str::to_string)
        .unwrap_or_else(|| local_daemon_hook_url(config))
}

/// The web port a hook URL assumes when `listen.web_port` is unset. `startup` refuses to serve
/// without that setting, so this only covers a config the daemon would not have started from — but
/// building the URL is not the place to discover it.
pub(super) const DEFAULT_WEB_PORT: u16 = 8899;

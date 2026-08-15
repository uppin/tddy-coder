//! Credential resolution: every parameter is a CLI flag with an environment fallback, so one
//! `GIT_SSH_COMMAND` (or a plain exported environment) serves every remote.
//!
//! There are only two of them — the daemon's address and one daemon token. The LiveKit room, URL
//! and JWT all come back from `auth.LiveKitTokenService/MintLiveKitToken`, which is what keeps
//! `LIVEKIT_API_SECRET` off this side of the wire: that secret is also the HMAC key every daemon
//! signs session tokens with, so a client holding it could mint an access token for any GitHub
//! user on the fleet.
//!
//! See docs/ft/daemon/remote-git-repo.md § Credentials.

use std::collections::HashMap;
use std::time::Duration;

/// Default wait for the daemon's participant to appear in the room.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// The daemon credential this client presents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonToken {
    /// An access token — used as-is. Lives 5 minutes.
    Access(String),
    /// A refresh token — exchanged for an access token via `auth.AuthService/RefreshSession`
    /// before anything else is done. Lives 7 days, which is what makes it usable from a CLI.
    Refresh(String),
}

/// Everything needed to reach a daemon and authenticate to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    /// Base URL of the daemon's Connect-HTTP surface — the one `/rpc/...` is appended to.
    pub daemon_url: String,
    pub token: DaemonToken,
    pub connect_timeout: Duration,
}

/// The flags as parsed, before environment fallback.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CredentialArgs {
    pub daemon_url: Option<String>,
    pub session_token: Option<String>,
    pub refresh_token: Option<String>,
    pub connect_timeout_secs: Option<u64>,
}

/// Why credentials could not be assembled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialError {
    /// A required parameter was given neither as a flag nor in the environment.
    Missing {
        flag: &'static str,
        env_var: &'static str,
    },
    /// A parameter was supplied but could not be read as the value it must be.
    Malformed {
        flag: &'static str,
        env_var: &'static str,
        value: String,
    },
    /// Neither `--session-token` nor `--refresh-token` was given.
    MissingToken,
}

impl CredentialError {
    /// `255` — ssh's transport/setup failure code, which is what git reports for an unreachable
    /// remote. Distinct from `128`, which means the request itself was malformed.
    pub fn exit_code(&self) -> i32 {
        255
    }
}

impl std::fmt::Display for CredentialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CredentialError::Missing { flag, env_var } => write!(
                f,
                "missing {flag}: pass it, or set the {env_var} environment variable"
            ),
            CredentialError::Malformed {
                flag,
                env_var,
                value,
            } => write!(
                f,
                "cannot read \"{value}\" as a value for {flag} ({env_var})"
            ),
            CredentialError::MissingToken => write!(
                f,
                "missing a daemon credential: pass --session-token (TDDY_SESSION_TOKEN) \
                 or --refresh-token (TDDY_REFRESH_TOKEN)"
            ),
        }
    }
}

/// Resolve credentials from flags, falling back to `env` per parameter. An access token wins over
/// a refresh token when both are supplied: it needs no exchange round trip.
pub fn resolve_credentials(
    args: &CredentialArgs,
    env: &HashMap<String, String>,
) -> Result<Credentials, CredentialError> {
    let configured = |flag: &Option<String>, env_var: &str| -> Option<String> {
        flag.clone().or_else(|| env.get(env_var).cloned())
    };

    let daemon_url =
        configured(&args.daemon_url, "TDDY_DAEMON_URL").ok_or(CredentialError::Missing {
            flag: "--daemon-url",
            env_var: "TDDY_DAEMON_URL",
        })?;

    let access = configured(&args.session_token, "TDDY_SESSION_TOKEN");
    let refresh = configured(&args.refresh_token, "TDDY_REFRESH_TOKEN");
    let token = match (access, refresh) {
        (Some(access), _) => DaemonToken::Access(access),
        (None, Some(refresh)) => DaemonToken::Refresh(refresh),
        (None, None) => return Err(CredentialError::MissingToken),
    };

    // The environment leg is parsed here rather than by clap so an unreadable value is reported
    // with the same "flag or variable" wording as everything else, and exits 255 like every other
    // setup failure. Falling back to the default instead would make a client wait 30s when the
    // operator asked for 2 — a fault they would have no way to notice.
    let connect_timeout = match (
        args.connect_timeout_secs,
        env.get("TDDY_CONNECT_TIMEOUT_SECS"),
    ) {
        (Some(secs), _) => Duration::from_secs(secs),
        (None, Some(raw)) => {
            let secs = raw.parse().map_err(|_| CredentialError::Malformed {
                flag: "--connect-timeout-secs",
                env_var: "TDDY_CONNECT_TIMEOUT_SECS",
                value: raw.clone(),
            })?;
            Duration::from_secs(secs)
        }
        (None, None) => DEFAULT_CONNECT_TIMEOUT,
    };

    Ok(Credentials {
        daemon_url,
        token,
        connect_timeout,
    })
}

//! Flags with per-parameter environment fallback, and the repo-root `.env` beneath both.
//!
//! Two credential sets, and that is not redundancy: LiveKit admits the syncer to the session room,
//! the daemon token authorizes the RPCs it makes there. See
//! `docs/ft/daemon/session-worktree-sync.md` § Credentials for why this client holds a LiveKit
//! secret when `tddy-remote-git-repo` deliberately holds none.
//!
//! Resolution is a pure function over an injected map rather than a reader of the process
//! environment, so every rule below is unit-testable without `set_var`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// How long to wait for the room, the participant and each RPC. Matches `tddy-remote-git-repo`.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// A daemon credential. An **access** token lives 5 minutes, too short for something configured
/// once, so a 7-day **refresh** token is accepted and exchanged before anything else runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonToken {
    Access(String),
    Refresh(String),
}

/// What admits the syncer to `session-{session_id}`.
///
/// `MintLiveKitToken` grants the daemon's **common room** and only that room, so a client that must
/// be in a session room has to mint for itself. Recorded as a widening of the trust surface in the
/// PRD rather than hidden here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveKitCredentials {
    pub url: String,
    pub api_key: String,
    pub api_secret: String,
}

/// Everything resolved, with nothing left to default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub session_id: String,
    pub dest: PathBuf,
    pub livekit: LiveKitCredentials,
    pub daemon_url: String,
    pub token: DaemonToken,
    pub connect_timeout: Duration,
}

/// The raw flags, before environment fallback. `None` means "not given on the command line".
#[derive(Debug, Clone, Default)]
pub struct SyncArgs {
    pub session_id: Option<String>,
    pub dest: Option<PathBuf>,
    pub livekit_url: Option<String>,
    pub livekit_api_key: Option<String>,
    pub livekit_api_secret: Option<String>,
    pub daemon_url: Option<String>,
    pub session_token: Option<String>,
    pub refresh_token: Option<String>,
    pub connect_timeout_secs: Option<String>,
}

/// Why a credential set could not be resolved.
///
/// Every variant names both the flag and the environment variable, because a user who set one is
/// usually reaching for the other, and a message naming only one sends them to the wrong place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialError {
    /// A required parameter was given by neither flag nor environment.
    Missing { flag: String, env_var: String },
    /// A parameter was given but cannot be read as what it must be. Never replaced by a default —
    /// silently substituting one is how a misconfigured run looks like a working one.
    Malformed {
        flag: String,
        env_var: String,
        reason: String,
    },
    /// Neither `--session-token` nor `--refresh-token`.
    MissingToken,
    /// `--dest` was not given. Its own variant rather than a [`CredentialError::Missing`] with an
    /// empty `env_var`, because it is the one parameter with no environment variable at all: the
    /// directory the syncer takes ownership of and discards local edits under is spelled out on
    /// the command line or not at all, never inherited from a shell that was set up for a
    /// different run.
    MissingDest,
}

impl CredentialError {
    /// Always non-zero, and always the same non-zero: a caller distinguishes *which* credential
    /// failed by reading the message, not by switching on a code.
    pub fn exit_code(&self) -> i32 {
        2
    }
}

impl std::fmt::Display for CredentialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CredentialError::Missing { flag, env_var } => {
                write!(f, "missing {flag} (or set {env_var})")
            }
            CredentialError::Malformed {
                flag,
                env_var,
                reason,
            } => write!(f, "invalid {flag} (or {env_var}): {reason}"),
            CredentialError::MissingToken => write!(
                f,
                "missing --session-token or --refresh-token \
                 (or set TDDY_SESSION_TOKEN or TDDY_REFRESH_TOKEN)"
            ),
            CredentialError::MissingDest => write!(
                f,
                "missing --dest: the directory to mirror has no environment variable \
                 and must be given on the command line"
            ),
        }
    }
}

impl std::error::Error for CredentialError {}

/// Resolve flags against `env`, which the binary fills from the process environment layered over
/// the repo-root `.env`.
///
/// An access token beats a refresh token when both are present: exchanging a refresh token costs a
/// round trip that an access token has already paid for.
pub fn resolve_credentials(
    args: &SyncArgs,
    env: &HashMap<String, String>,
) -> Result<Credentials, CredentialError> {
    let session_id = required(
        args.session_id.as_deref(),
        "--session-id",
        SESSION_ID_ENV,
        env,
    )?;
    let dest = args.dest.clone().ok_or(CredentialError::MissingDest)?;

    let livekit = LiveKitCredentials {
        url: required(
            args.livekit_url.as_deref(),
            "--livekit-url",
            LIVEKIT_URL_ENV,
            env,
        )?,
        api_key: required(
            args.livekit_api_key.as_deref(),
            "--livekit-api-key",
            LIVEKIT_API_KEY_ENV,
            env,
        )?,
        api_secret: required(
            args.livekit_api_secret.as_deref(),
            "--livekit-api-secret",
            LIVEKIT_API_SECRET_ENV,
            env,
        )?,
    };

    let daemon_url = required(
        args.daemon_url.as_deref(),
        "--daemon-url",
        DAEMON_URL_ENV,
        env,
    )?;

    // An access token beats a refresh token: the exchange a refresh token needs is a round trip
    // the access token has already paid for.
    let token = match (
        resolve(args.session_token.as_deref(), SESSION_TOKEN_ENV, env),
        resolve(args.refresh_token.as_deref(), REFRESH_TOKEN_ENV, env),
    ) {
        (Some(access), _) => DaemonToken::Access(access),
        (None, Some(refresh)) => DaemonToken::Refresh(refresh),
        (None, None) => return Err(CredentialError::MissingToken),
    };

    let connect_timeout = match resolve(
        args.connect_timeout_secs.as_deref(),
        CONNECT_TIMEOUT_SECS_ENV,
        env,
    ) {
        None => DEFAULT_CONNECT_TIMEOUT,
        // Parsed here rather than by clap so an unparsable value is refused. Clap would report
        // it too, but only for the flag — a malformed environment variable would go to the
        // default and a run that never connected would look exactly like a working one.
        Some(raw) => {
            Duration::from_secs(raw.parse::<u64>().map_err(|e| CredentialError::Malformed {
                flag: "--connect-timeout-secs".to_string(),
                env_var: CONNECT_TIMEOUT_SECS_ENV.to_string(),
                reason: e.to_string(),
            })?)
        }
    };

    Ok(Credentials {
        session_id,
        dest,
        livekit,
        daemon_url,
        token,
        connect_timeout,
    })
}

/// The environment variable behind each flag. Named constants because the flag, the `#[arg(env)]`
/// on the CLI and the message a refusal prints must all say the same word: a variable each spelled
/// for itself fails as a run that silently ignores what the user exported.
const SESSION_ID_ENV: &str = "TDDY_SESSION_ID";
const LIVEKIT_URL_ENV: &str = "LIVEKIT_URL";
const LIVEKIT_API_KEY_ENV: &str = "LIVEKIT_API_KEY";
const LIVEKIT_API_SECRET_ENV: &str = "LIVEKIT_API_SECRET";
const DAEMON_URL_ENV: &str = "TDDY_DAEMON_URL";
const SESSION_TOKEN_ENV: &str = "TDDY_SESSION_TOKEN";
const REFRESH_TOKEN_ENV: &str = "TDDY_REFRESH_TOKEN";
const CONNECT_TIMEOUT_SECS_ENV: &str = "TDDY_CONNECT_TIMEOUT_SECS";

/// The flag if it was given, the environment variable otherwise.
///
/// The flag wins outright: an environment left over from another run must never redirect a command
/// the user spelled out.
fn resolve(flag: Option<&str>, env_var: &str, env: &HashMap<String, String>) -> Option<String> {
    match flag {
        Some(value) => Some(value.to_string()),
        // An exported-but-blank variable, and a `.env` line with nothing after the `=`, both mean
        // the developer cleared it — not that the URL is the empty string. Without this an empty
        // `LIVEKIT_URL` resolves and the run fails somewhere further in, describing a connection
        // rather than the setting that was never given.
        None => env
            .get(env_var)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    }
}

/// [`resolve`], for a parameter that has no default and therefore cannot be missing.
fn required(
    flag: Option<&str>,
    flag_name: &str,
    env_var: &str,
    env: &HashMap<String, String>,
) -> Result<String, CredentialError> {
    resolve(flag, env_var, env).ok_or_else(|| CredentialError::Missing {
        flag: flag_name.to_string(),
        env_var: env_var.to_string(),
    })
}

/// Parse `.env` content into key/value pairs.
///
/// Hand-rolled rather than a `dotenv` dependency, matching `tddy_vm_testkit::env_file`: the
/// semantics needed are the fifteen lines `./web-dev` already implements for the same file, and a
/// second reader that agreed with it by coincidence would drift.
///
/// Mirrors `IFS='=' read -r key value`: blank lines and `#` comments are skipped, the first `=`
/// separates key from value — a base64 secret ends in `=` and losing that byte is a credential that
/// fails to verify for no visible reason — and one layer of surrounding single or double quotes is
/// stripped. A key with an empty value is reported rather than dropped — deciding that empty means
/// unset is [`resolve_credentials`]'s job, and it treats an empty `.env` value and an exported-but-
/// blank variable the same way: as absent.
pub fn parse_env_file(contents: &str) -> Vec<(String, String)> {
    contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_string(), unquote(value).to_string()))
        })
        .collect()
}

/// The environment [`resolve_credentials`] reads: the process environment, with the repo-root
/// `.env` filling only the gaps it leaves.
///
/// **An already-set variable always wins.** That is the rule `./web-dev` and
/// `tddy_vm_testkit::env_file` both implement for this same file, and the reason is the same in all
/// three: a developer who exports `LIVEKIT_URL` for one run must not have it silently replaced by
/// whatever their `.env` happens to say, because the run would then succeed against the wrong
/// server and look exactly like the run they asked for.
///
/// An absent `.env` is not an error — it is gitignored and per-developer, so its absence just means
/// every variable comes from the environment.
pub fn layered_environment(
    process_env: HashMap<String, String>,
    env_file_contents: Option<&str>,
) -> HashMap<String, String> {
    let mut env = process_env;
    for (key, value) in env_file_contents.map(parse_env_file).unwrap_or_default() {
        env.entry(key).or_insert(value);
    }
    env
}

/// Strip one layer of matching surrounding quotes, as the shell would have.
fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|v| v.strip_suffix(quote))
        {
            return inner;
        }
    }
    value
}

//! `notify` subcommand: an agent notifying its operator in its own words.
//!
//! ```text
//! tddy-tools notify "Parallel run finished" --body "4 done, **1 blocked**" --level success --sound
//! ```
//!
//! The session it notifies about, and the credential it presents, come from the environment a
//! daemon-launched claude-cli or cursor-cli session's agent is given
//! ([`tddy_service::session_activity::notify_env`]). It calls `ReportOperatorNotification` over the
//! embedded daemon's agent-tool socket when `TDDY_DAEMON_SOCKET` is set, over `TDDY_DAEMON_URL`
//! otherwise.
//!
//! **Not fail-quiet**, unlike `session-hook`: an agent that asked to notify must learn that it did
//! not. A missing environment, an unreachable daemon or a refusal exits non-zero with the reason;
//! success prints `Notification sent`.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::{Args, ValueEnum};
use prost::Message as _;
use tddy_service::proto::activity::{NotificationLevel, ReportOperatorNotificationRequest};
use tddy_service::session_activity::{notify_env, ACTIVITY_SERVICE};

/// How long the daemon may take to answer. Longer than a hook's budget, since nothing here blocks
/// the agent's turn: it is waiting for this command's verdict.
const NOTIFY_TIMEOUT: Duration = Duration::from_secs(10);

/// The method this subcommand calls on [`ACTIVITY_SERVICE`].
const METHOD: &str = "ReportOperatorNotification";

/// How the agent grades what it is telling its operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum NotifyLevel {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Args)]
pub struct NotifyArgs {
    /// One-line title, 1–200 characters after trimming.
    pub title: String,

    /// Markdown body, at most 2,000 characters. `-` reads it from stdin.
    #[arg(long)]
    pub body: Option<String>,

    /// How the operator should read it.
    #[arg(long, value_enum, default_value_t = NotifyLevel::Info)]
    pub level: NotifyLevel,

    /// Alert the operator audibly. Without it the notification arrives silently where a surface
    /// can send one silently.
    #[arg(long)]
    pub sound: bool,
}

/// Send the notification `args` describes, for the session the environment names.
///
/// `Ok` only once the daemon accepted it; every other outcome is an `Err` naming why, which the
/// binary prints and exits non-zero on.
pub async fn run_notify(args: NotifyArgs) -> anyhow::Result<()> {
    let session_id = required_env(notify_env::SESSION_ID)?;
    let os_user = required_env(notify_env::OS_USER)?;
    let hook_token = required_env(notify_env::HOOK_TOKEN)?;
    let transport = Transport::from_env()?;

    let body_markdown = match args.body.as_deref() {
        Some("-") => read_stdin().await?,
        Some(body) => body.to_string(),
        None => String::new(),
    };
    let request = ReportOperatorNotificationRequest {
        session_id,
        hook_token,
        os_user,
        title: args.title,
        body_markdown,
        level: NotificationLevel::from(args.level) as i32,
        sound: args.sound,
    };

    transport.send(request.encode_to_vec()).await?;
    println!("Notification sent");
    Ok(())
}

impl From<NotifyLevel> for NotificationLevel {
    fn from(level: NotifyLevel) -> Self {
        match level {
            NotifyLevel::Info => NotificationLevel::Info,
            NotifyLevel::Success => NotificationLevel::Success,
            NotifyLevel::Warning => NotificationLevel::Warning,
            NotifyLevel::Error => NotificationLevel::Error,
        }
    }
}

/// The value of a session environment variable, or an error naming it.
fn required_env(name: &str) -> anyhow::Result<String> {
    match std::env::var(name) {
        Ok(value) if !value.is_empty() => Ok(value),
        _ => anyhow::bail!(
            "{name} is not set: `tddy-tools notify` runs inside a session the daemon launched"
        ),
    }
}

async fn read_stdin() -> anyhow::Result<String> {
    tokio::task::spawn_blocking(|| {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map(|_| buf)
            .map_err(|e| anyhow::anyhow!("read the body from stdin: {e}"))
    })
    .await
    .map_err(|e| anyhow::anyhow!("read the body from stdin: {e}"))?
}

/// How the daemon is reached: its agent-tool socket when the session was given one, which an
/// embedded daemon serves instead of HTTP, its URL otherwise.
enum Transport {
    Socket(PathBuf),
    Http(String),
}

impl Transport {
    fn from_env() -> anyhow::Result<Self> {
        if let Ok(socket) = std::env::var(notify_env::DAEMON_SOCKET) {
            if !socket.is_empty() {
                return Ok(Self::Socket(PathBuf::from(socket)));
            }
        }
        match std::env::var(notify_env::DAEMON_URL) {
            Ok(url) if !url.is_empty() => Ok(Self::Http(url)),
            _ => anyhow::bail!(
                "neither {} nor {} is set: the daemon cannot be reached",
                notify_env::DAEMON_SOCKET,
                notify_env::DAEMON_URL
            ),
        }
    }

    async fn send(&self, body: Vec<u8>) -> anyhow::Result<()> {
        match self {
            Self::Socket(socket) => call_over_socket(socket, body).await,
            Self::Http(url) => post_proto(url, body).await,
        }
    }
}

async fn call_over_socket(socket: &Path, body: Vec<u8>) -> anyhow::Result<()> {
    let context = format!("{ACTIVITY_SERVICE}/{METHOD} over {}", socket.display());
    let call = async {
        let client = tddy_session_tool_client::connect_sandbox_ipc(socket)
            .await
            .map_err(|e| anyhow::anyhow!("{context}: {e}"))?;
        client
            .call_unary(ACTIVITY_SERVICE, METHOD, body)
            .await
            .map_err(|e| anyhow::anyhow!("{context}: {e}"))
    };
    tokio::time::timeout(NOTIFY_TIMEOUT, call)
        .await
        .map_err(|_| anyhow::anyhow!("{context}: timed out"))??;
    Ok(())
}

/// POST the request over the Connect protocol. A refusal's body carries the daemon's reason, so
/// it is passed on rather than reduced to a status code.
async fn post_proto(daemon: &str, body: Vec<u8>) -> anyhow::Result<()> {
    let url = format!(
        "{}/rpc/{ACTIVITY_SERVICE}/{METHOD}",
        daemon.trim_end_matches('/')
    );
    let client = reqwest::Client::builder()
        .timeout(NOTIFY_TIMEOUT)
        .build()
        .map_err(|e| anyhow::anyhow!("build http client: {e}"))?;
    let response = client
        .post(&url)
        .header("content-type", "application/proto")
        .body(body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("POST {url}: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        let reason = response.text().await.unwrap_or_default();
        anyhow::bail!("POST {url} → HTTP {status}: {reason}");
    }
    Ok(())
}

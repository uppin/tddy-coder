//! What this daemon process calls itself.
//!
//! Two ids, deliberately distinct, and the distinction is load-bearing:
//!
//! - the **routing** id ([`local_instance_id_for_config`]) is what this process answers to in the
//!   common room, made unique per *run* when `daemon_instance_id_append_startup_timestamp` is set;
//! - the **durable** id ([`local_base_instance_id_for_config`]) is what anything outliving the
//!   process must key on — the host registry above all.
//!
//! They live here rather than in the peer-discovery module they were derived in because the spawn
//! layer, the peer-discovery layer and the host subsystem all need them, and those are now in three
//! different crates. `livekit_peer_discovery` and `multi_host` re-export them, so no caller's path
//! changed.

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::DaemonConfig;

/// The local machine's default daemon instance id — its short hostname when there is one.
#[must_use]
pub fn local_daemon_instance_id_string() -> String {
    #[cfg(unix)]
    {
        let mut buf = [0u8; 256];
        let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
        if rc != 0 {
            log::debug!(
                "local_daemon_instance_id_string: gethostname failed rc={}",
                rc
            );
            return "local".to_string();
        }
        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        let s = std::str::from_utf8(&buf[..len])
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "local".to_string());
        if s.is_empty() {
            "local".to_string()
        } else {
            s
        }
    }
    #[cfg(not(unix))]
    {
        "local".to_string()
    }
}

/// The daemon's **durable** identity: the configured instance id, or the hostname, without the
/// per-process startup suffix.
///
/// This is the id anything that outlives the process must key on — the host registry above all.
/// `daemon_instance_id_append_startup_timestamp` makes the routing id unique per *run*, so keying
/// durable state on it would file every restart of one machine as a brand-new host, and the host
/// registry's "never delete a host" rule would then keep every one of those ghosts forever.
///
/// Derived here once so no caller is tempted to recover it by stripping a suffix off the routing
/// id: `server-2` and `server-<startup ms>` are indistinguishable to a string match, and getting
/// that wrong renames a real host.
#[must_use]
pub fn local_base_instance_id_for_config(config: &DaemonConfig) -> String {
    config
        .daemon_instance_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(local_daemon_instance_id_string)
}

/// Resolved local daemon instance id string (config override or hostname default).
///
/// This is the **routing** identity: the id this process answers to in the common room and in
/// `classify_peer_route`. See [`local_base_instance_id_for_config`] for the durable one.
#[must_use]
pub fn local_instance_id_for_config(config: &DaemonConfig) -> String {
    let base = local_base_instance_id_for_config(config);
    if config.daemon_instance_id_append_startup_timestamp {
        format!("{}-{}", base, process_startup_unix_ms_suffix())
    } else {
        base
    }
}

/// The wall-clock millisecond this process started, as a string, computed once.
#[must_use]
pub fn process_startup_unix_ms_suffix() -> &'static str {
    static SUFFIX: OnceLock<String> = OnceLock::new();
    SUFFIX
        .get_or_init(|| {
            let ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            format!("{ms}")
        })
        .as_str()
}

/// Identity prefix reserved for split sessions' agent participants.
///
/// Reserved, not merely conventional: peer eligibility is decided from self-declared participant
/// metadata, so this prefix is what
/// `livekit_peer_discovery::eligible_daemon_from_participant_fields` matches on to refuse an agent
/// advertising itself as a daemon. A daemon whose `daemon_instance_id` began with it would not be
/// discoverable — which is the intended trade, since the agent holds a token it can publish
/// metadata with and the daemon's instance id is an operator's free choice.
///
/// It lives here with the two daemon ids rather than in `split_session`, which mints identities
/// from it, because the module that *refuses* them is now in another crate. `split_session`
/// re-exports it, so no caller's path changed and there stays exactly one definition.
pub const SPLIT_AGENT_IDENTITY_PREFIX: &str = "split-agent-";

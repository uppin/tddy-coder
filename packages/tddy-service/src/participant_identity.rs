//! What a LiveKit participant identity says about who holds it — the one rule both sides of the
//! common room read.
//!
//! A daemon's *discovery* participant joins the common room under its bare instance id, an
//! operator's free choice, and publishes an advertisement as its metadata — carrying, among other
//! things, the public key it signs session tokens with. A peer daemon trusts that key. So the
//! question "could this participant be a daemon?" is asked in two places that must never disagree:
//!
//! - **peer discovery** (`tddy_daemon_livekit::livekit_peer_discovery`) refuses to take any
//!   participant for a daemon unless [`may_be_daemon_discovery_identity`] allows it;
//! - **every client-facing mint** (`token.TokenService`, [`crate::TokenServiceImpl`]) refuses to
//!   hand out an identity [`may_be_daemon_discovery_identity`] allows.
//!
//! Together they make an advertised key trustworthy by construction: the only participants
//! discovery reads a key from are ones holding an identity no client can be minted, which means an
//! identity a daemon minted for itself. Deciding it in one function is what stops the two drifting —
//! a prefix added to one list and not the other would reopen the forgery the pair exists to close.

/// Prefix of the LiveKit identity a daemon **serves** its RPC on —
/// `tddy_daemon_kernel::peer_forwarding::daemon_rpc_identity` composes its identities from this
/// constant so the two cannot drift.
///
/// No client-facing mint hands out an identity carrying it, on any registration. A participant
/// admitted to a room under a `daemon-*` identity is handed the RPC calls other participants
/// address to that daemon, so a caller free to choose it would be reading everyone else's traffic.
/// It is also never a *discovery* identity: a session's coder participant joins under
/// `daemon-<uuid>…` and is not a host daemon.
pub const RESERVED_DAEMON_IDENTITY_PREFIX: &str = "daemon-";

/// Identity prefix reserved for split sessions' agent participants.
///
/// Reserved, not merely conventional: an agent holds a join token it can publish metadata with,
/// and runs model-authored code, so this prefix is what keeps it from advertising itself as a
/// daemon. A daemon whose `daemon_instance_id` began with it would not be discoverable — the
/// intended trade, since the daemon's instance id is an operator's free choice.
///
/// `tddy_daemon_kernel::daemon_identity` re-exports it for the crates that mint agents' identities.
pub const SPLIT_AGENT_IDENTITY_PREFIX: &str = "split-agent-";

/// Prefix of the identity `auth.LiveKitTokenService/MintLiveKitToken` generates for a
/// `tddy-remote-git-repo` client admitted to the common room. Deliberately neither `daemon-` nor a
/// bare id: that mint's JWT may update its own metadata, so a client under an identity discovery
/// read would be a client able to advertise a signing key.
pub const REMOTE_GIT_IDENTITY_PREFIX: &str = "remote-git-";

/// Prefix of the identity a daemon's screen-share bridge joins the common room under
/// (`screenshare-host-<instance>-<target>`). Its room token may update its own metadata, so
/// discovery must not read an advertisement from it.
pub const SCREEN_SHARE_HOST_IDENTITY_PREFIX: &str = "screenshare-host-";

/// Prefixes of the identities a browser joins a room under — dashboard presence (`web-…`) and the
/// presenter's room (`browser-…`). The only identities a web client needs from `token.TokenService`.
pub const BROWSER_IDENTITY_PREFIXES: [&str; 2] = ["web-", "browser-"];

/// Prefix of a session coder's participant identity (`server`, `server-…`).
pub const CODER_IDENTITY_PREFIX: &str = "server";

/// Every identity prefix that is never a daemon's discovery participant.
///
/// Mirrors the web UI's `inferParticipantRole` (`tddy-web/src/lib/participantRole.ts`), which
/// sorts the same prefixes into browser and coder rows; the remote-git and split-agent prefixes are
/// not rows it draws, and it never trusts a key.
pub const NON_DAEMON_IDENTITY_PREFIXES: [&str; 7] = [
    BROWSER_IDENTITY_PREFIXES[0],
    BROWSER_IDENTITY_PREFIXES[1],
    CODER_IDENTITY_PREFIX,
    RESERVED_DAEMON_IDENTITY_PREFIX,
    SPLIT_AGENT_IDENTITY_PREFIX,
    REMOTE_GIT_IDENTITY_PREFIX,
    SCREEN_SHARE_HOST_IDENTITY_PREFIX,
];

/// Whether a participant under `identity` could be a daemon's discovery participant — the one
/// whose advertisement, signing key included, peer daemons believe.
///
/// `true` for every identity that does not begin with one of [`NON_DAEMON_IDENTITY_PREFIXES`].
/// Compared after trimming, as discovery reads identities, and case-sensitively, as LiveKit routes
/// them: `Web-x` is *not* a browser identity, so a mint refuses it rather than let discovery read it.
pub fn may_be_daemon_discovery_identity(identity: &str) -> bool {
    let identity = identity.trim();
    !NON_DAEMON_IDENTITY_PREFIXES
        .iter()
        .any(|prefix| identity.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_instance_id_may_be_a_daemon() {
        // Given the identity a daemon's discovery participant joins under — its bare instance id
        // When / Then
        assert!(may_be_daemon_discovery_identity("udoo-1780828020298"));
    }

    #[test]
    fn no_identity_under_a_non_daemon_prefix_may_be_a_daemon() {
        // Given one identity under each prefix discovery never takes for a daemon
        let identities = [
            "web-alice",
            "browser-presenter-x1",
            "server",
            "server-7",
            "daemon-udoo",
            "split-agent-s1",
            "remote-git-0b6f",
            "screenshare-host-udoo-display-1",
        ];

        // When each is classified
        let daemon_eligible: Vec<&str> = identities
            .into_iter()
            .filter(|identity| may_be_daemon_discovery_identity(identity))
            .collect();

        // Then none may be a daemon
        assert_eq!(daemon_eligible, Vec::<&str>::new());
    }

    #[test]
    fn padding_does_not_change_the_answer() {
        // Given a browser identity behind whitespace, as a caller may send it
        // When / Then it is still a browser's — discovery trims before it reads
        assert!(!may_be_daemon_discovery_identity("  web-alice "));
    }

    #[test]
    fn a_prefix_in_another_case_is_not_that_prefix() {
        // Given a browser prefix spelled in another case
        // When / Then it is not a browser identity, so it may be a daemon — and a mint refuses it
        assert!(may_be_daemon_discovery_identity("Web-alice"));
    }
}

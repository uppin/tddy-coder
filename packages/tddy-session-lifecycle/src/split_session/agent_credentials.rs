use tddy_github::{SessionTokenSigner, TokenKind};

use tddy_github::SessionTokenError;

use tddy_github::GitHubUser;

use super::SPLIT_AGENT_TOKEN_TTL;

use tddy_rpc::Status;

use tddy_daemon_auth::SessionTokens;

/// Mint the agent's own session token from the caller's, refusing anything the caller could not
/// legitimately have presented.
///
/// The web caller's access token lives [`tddy_github::SESSION_TOKEN_TTL`] — five minutes — because
/// the browser holds a refresh token and re-mints it long before expiry. A spawned agent holds
/// neither: whatever life was left on the caller's token when the session started is all its
/// `tddy-tools --mcp` child would ever have, and every remote tool call after that fails
/// `UNAUTHENTICATED` on the codebase daemon with nothing on this host to notice. So the agent is
/// given a credential of its own, scoped to the same [`SPLIT_AGENT_TOKEN_TTL`] as the join token
/// minted beside it — one agent process's working life, not the session's.
///
/// It is minted under the *verified* caller's identity, never the claimed one: the login in these
/// claims is what the codebase daemon looks up in its own `users[]` table to pick the OS user the
/// tools run as, so a token this daemon could not verify would be this daemon choosing a user on
/// another host's behalf. It is signed with this daemon's own key, which the codebase daemon
/// resolves from this daemon's common-room advertisement — which is exactly why it will accept
/// what is minted here. Every failure is a refusal rather than a fallback to forwarding the
/// caller's token: an expired, forged or malformed credential must not buy a session-length one.
pub(crate) fn mint_agent_session_token(
    tokens: &SessionTokens,
    caller_token: &str,
) -> Result<String, Status> {
    let caller = verified_caller(tokens, caller_token)?;
    Ok(tokens.signer().mint(&caller, SPLIT_AGENT_TOKEN_TTL))
}

/// The identity a caller's token *proves*, or a refusal.
///
/// Shared by everything a split session start signs under the caller, because each of them
/// (the agent's own token, the room poller's per-poll credential) is this daemon asserting an
/// identity to another host: the login in the claims is what the codebase daemon looks up in its
/// own `users[]` table to pick the OS user, so a token this daemon could not verify would be this
/// daemon choosing a user on another host's behalf. Every failure is a refusal rather than a
/// fallback to forwarding the caller's token: an expired, forged or malformed credential must not
/// buy a minted one.
fn verified_caller(tokens: &SessionTokens, caller_token: &str) -> Result<GitHubUser, Status> {
    let refused =
        |why: String| Status::unauthenticated(format!("cannot wire a split session: {why}"));
    let claims = tokens.verifier().verify_now(caller_token).map_err(|e| {
        refused(match e {
            SessionTokenError::Expired => "the caller's session token has expired".to_string(),
            SessionTokenError::InvalidSignature => {
                "the caller's session token does not verify under the key it names".to_string()
            }
            SessionTokenError::Malformed => "the caller's session token is malformed".to_string(),
            SessionTokenError::UnsupportedVersion => "the caller's session token is in a \
                     format this daemon no longer accepts — sign in again"
                .to_string(),
            SessionTokenError::UnknownKeyId(key_id) => format!(
                "the caller's session token is signed by key {key_id}, which no daemon this \
                     one knows has published"
            ),
        })
    })?;
    // A refresh token mints access tokens and never authenticates an RPC (see [`TokenKind`]), so
    // accepting one here would let the credential a browser keeps at rest authorize a whole
    // session's toolchain on the codebase host.
    if claims.kind == TokenKind::Refresh {
        return Err(refused(
            "the caller presented a refresh token, which never authenticates an RPC".to_string(),
        ));
    }
    Ok(claims.user())
}

/// The credential the facilitating daemon's room poller presents to the codebase daemon, minted
/// fresh for every poll.
///
/// The room asks the codebase daemon for a worktree snapshot on a timer, and that peer
/// authenticates each one exactly as it authenticates a tool call. Holding the caller's token for
/// that would give the room five minutes ([`tddy_github::SESSION_TOKEN_TTL`]) of working life and
/// then a silent, permanent `Unauthenticated`. Unlike the agent — which lives in another process
/// and has to be handed something up front (see [`mint_agent_session_token`]) — the poller runs
/// inside the daemon that holds the signing key, so it keeps the *identity* and signs a
/// short-lived token per poll: no expiry ceiling on the room, and no long-lived bearer token at
/// rest in this process.
pub struct RoomPollTokenMinter {
    pub(crate) signer: SessionTokenSigner,
    /// The verified caller, never the claimed one — [`verified_caller`].
    pub(crate) caller: GitHubUser,
}

impl RoomPollTokenMinter {
    /// Verify the caller once, here, so a session whose room could never authenticate anything
    /// fails to start rather than starting and then measuring nothing forever.
    pub fn new(tokens: &SessionTokens, caller_token: &str) -> Result<Self, Status> {
        let caller = verified_caller(tokens, caller_token)?;
        Ok(Self {
            signer: tokens.signer().clone(),
            caller,
        })
    }
}

impl tddy_daemon_livekit::session_room::SessionTokenMinter for RoomPollTokenMinter {
    /// [`tddy_github::SESSION_TOKEN_TTL`] and no longer: a poll that outlives its own credential is
    /// the bug this exists to remove, and the next poll mints another.
    fn mint(&self) -> String {
        self.signer.mint_access(&self.caller)
    }
}

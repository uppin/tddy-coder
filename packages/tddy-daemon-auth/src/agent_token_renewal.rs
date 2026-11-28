//! Renewing a session agent's own token before it runs out.
//!
//! An agent's token lives [`tddy_github::AGENT_TOKEN_TTL`] and the agent holds no refresh token.
//! Before this existed, an agent that outlived one TTL lost every `tddy-tools` call at once. An
//! orchestrator has no shell, so it had no way out (session `01a20f10`, 2026-11-27). Its
//! `tddy-tools --mcp` now renews the token over the daemon socket while it is still valid.
//!
//! Renewal is deliberately narrow. It accepts only a still-valid agent access token that **this**
//! daemon signed, for the session it names, and it returns a token with the same identity,
//! principal and session. An expired token is refused: renewal extends a credential, it never
//! revives one.

use std::time::{SystemTime, UNIX_EPOCH};

use tddy_github::{SessionTokenError, TokenKind, TokenPrincipal, AGENT_TOKEN_TTL};

use crate::signing_key::SessionTokens;

/// A renewed agent token and the moment it expires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenewedAgentToken {
    pub session_token: String,
    pub expires_at_unix: u64,
}

/// Why a token was not renewed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentTokenRenewalRefusal {
    /// The token did not verify: expired, forged, malformed or signed by an unknown key.
    NotVerified(SessionTokenError),
    /// Signed by another daemon. Only the signer renews, so a codebase host cannot extend a token
    /// an agent host issued.
    NotSignedHere,
    /// Not an agent's access token: a user's, a relay's, or a refresh token.
    NotAnAgentToken,
    /// Minted for a different session than the one the renewal names.
    WrongSession,
}

impl std::fmt::Display for AgentTokenRenewalRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotVerified(SessionTokenError::Expired) => f.write_str(
                "the token has already expired; an expired agent token is replaced only when the \
                 daemon resumes the session",
            ),
            Self::NotVerified(e) => write!(f, "the token did not verify: {e}"),
            Self::NotSignedHere => f.write_str("the token was signed by another daemon"),
            Self::NotAnAgentToken => f.write_str("the token is not an agent's access token"),
            Self::WrongSession => f.write_str("the token was minted for another session"),
        }
    }
}

impl SessionTokens {
    /// Renew `current`, an agent token for `session_id`, into a fresh one valid for
    /// [`AGENT_TOKEN_TTL`] from now.
    pub fn renew_agent(
        &self,
        current: &str,
        session_id: &str,
    ) -> Result<RenewedAgentToken, AgentTokenRenewalRefusal> {
        let claims = self
            .verifier()
            .verify_now(current)
            .map_err(AgentTokenRenewalRefusal::NotVerified)?;
        if &claims.kid != self.signer().key_id() {
            return Err(AgentTokenRenewalRefusal::NotSignedHere);
        }
        if claims.kind != TokenKind::Access || claims.principal != Some(TokenPrincipal::Agent) {
            return Err(AgentTokenRenewalRefusal::NotAnAgentToken);
        }
        if claims.session_id.as_deref() != Some(session_id) {
            return Err(AgentTokenRenewalRefusal::WrongSession);
        }
        let session_token = self
            .signer()
            .mint_agent(&claims.user(), AGENT_TOKEN_TTL, session_id);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        Ok(RenewedAgentToken {
            session_token,
            expires_at_unix: now + AGENT_TOKEN_TTL.as_secs(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use tddy_github::{GitHubUser, SessionTokenVerifier};

    use super::*;
    use crate::signing_key::{DaemonSigningKey, StandaloneKeyDirectory};

    const SESSION: &str = "01a20f10-orchestrator";

    fn an_operator() -> GitHubUser {
        GitHubUser {
            id: 7,
            login: "operator".to_string(),
            avatar_url: String::new(),
            name: "Operator".to_string(),
        }
    }

    fn a_daemon() -> (SessionTokens, tempfile::TempDir) {
        let home = tempfile::tempdir().expect("a data directory");
        let key = DaemonSigningKey::load_or_generate(&home.path().join("signing.key"))
            .expect("a signing key");
        (
            SessionTokens::new(&key, Arc::new(StandaloneKeyDirectory)),
            home,
        )
    }

    fn expiry_of(token: &str) -> u64 {
        SessionTokenVerifier::unverified_expiry(token).expect("a token with an expiry")
    }

    #[test]
    fn an_agent_token_near_expiry_is_renewed_to_a_full_ttl_past_its_old_expiry() {
        // Given an agent token with one minute left
        let (daemon, _home) = a_daemon();
        let near_expiry =
            daemon
                .signer()
                .mint_agent(&an_operator(), Duration::from_secs(60), SESSION);

        // When its tddy-tools renews it
        let renewed = daemon
            .renew_agent(&near_expiry, SESSION)
            .expect("a valid agent token is renewed");

        // Then the new token outlives the old one by about a full TTL and still verifies as the same agent
        assert!(expiry_of(&renewed.session_token) >= expiry_of(&near_expiry) + 86_400 - 120);
        assert_eq!(expiry_of(&renewed.session_token), renewed.expires_at_unix);
        let claims = daemon
            .verifier()
            .verify_now(&renewed.session_token)
            .expect("the renewed token verifies");
        assert_eq!(
            (
                claims.login.as_str(),
                claims.principal,
                claims.session_id.as_deref()
            ),
            ("operator", Some(TokenPrincipal::Agent), Some(SESSION))
        );
    }

    #[test]
    fn an_expired_agent_token_is_not_revived() {
        let (daemon, _home) = a_daemon();
        let expired = daemon.signer().mint_agent_with_issued_at(
            &an_operator(),
            SystemTime::now() - Duration::from_secs(90_000),
            AGENT_TOKEN_TTL,
            SESSION,
        );

        assert_eq!(
            daemon.renew_agent(&expired, SESSION),
            Err(AgentTokenRenewalRefusal::NotVerified(
                SessionTokenError::Expired
            ))
        );
    }

    #[test]
    fn a_users_token_cannot_buy_an_agent_token() {
        let (daemon, _home) = a_daemon();
        let user = daemon
            .signer()
            .mint(&an_operator(), Duration::from_secs(600));

        assert_eq!(
            daemon.renew_agent(&user, SESSION),
            Err(AgentTokenRenewalRefusal::NotAnAgentToken)
        );
    }

    #[test]
    fn an_agent_token_renews_only_for_its_own_session() {
        let (daemon, _home) = a_daemon();
        let token = daemon
            .signer()
            .mint_agent(&an_operator(), Duration::from_secs(600), SESSION);

        assert_eq!(
            daemon.renew_agent(&token, "another-session"),
            Err(AgentTokenRenewalRefusal::WrongSession)
        );
    }

    #[test]
    fn another_daemons_agent_token_is_not_renewed_here() {
        let (signer_daemon, _a) = a_daemon();
        let (other_daemon, _b) = a_daemon();
        let token =
            signer_daemon
                .signer()
                .mint_agent(&an_operator(), Duration::from_secs(600), SESSION);

        let refusal = other_daemon
            .renew_agent(&token, SESSION)
            .expect_err("a foreign token is refused");

        // The standalone directory knows no peer key, so the token fails verification first.
        assert!(matches!(
            refusal,
            AgentTokenRenewalRefusal::NotVerified(SessionTokenError::UnknownKeyId(_))
                | AgentTokenRenewalRefusal::NotSignedHere
        ));
    }
}

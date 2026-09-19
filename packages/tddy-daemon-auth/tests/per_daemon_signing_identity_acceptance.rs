//! One daemon's session token authenticates its holder on a *different* daemon, with no secret
//! shared between the two.
//!
//! This is the whole point of the per-daemon signing identity. Today every daemon in a deployment
//! verifies tokens by holding one `livekit.api_secret`, which means the fleet's session
//! authentication and its media credentials are the same secret — rotate one and you have silently
//! rotated the other, and a daemon that serves no media cannot authenticate anybody. With a keypair
//! per daemon, A signs with a key only A holds, B verifies with A's *public* half, and neither has
//! to be told anything the other must keep quiet.
//!
//! What makes that safe is the refusal: a token naming a key B has never seen is rejected outright.
//! There is no permissive stand-in, because a fallback here would accept forged tokens from anybody
//! for exactly as long as nobody looked.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use ed25519_dalek::VerifyingKey;
use tddy_daemon_auth::{DaemonSigningKey, DirectorySessionTokenVerifier, KeyDirectory};
use tddy_github::session_token_v2::{KeyId, SessionTokenError};
use tddy_github::GitHubUser;

const THE_LOGIN: &str = "operator";

#[tokio::test]
async fn a_token_daemon_a_minted_is_admitted_by_daemon_b_once_b_has_seen_as_key() {
    // Given daemon A, which signed a session token for an operator with its own key
    let fleet = a_common_key_directory();
    let daemon_a = a_daemon_that_has_published_its_key(&fleet).await;
    let token = daemon_a.signer().mint_access(&an_operator());

    // And daemon B, which holds a different key and no secret of A's
    let daemon_b = a_daemon_that_has_published_its_key(&fleet).await;

    // When B verifies the token A minted
    let claims = the_verifier_for(&daemon_b, &fleet).verify(&token).await;

    // Then B admits it, as the operator A signed it for
    assert_eq!(
        claims.map(|claims| claims.login),
        Ok(THE_LOGIN.to_string()),
        "a daemon must verify a peer's token from the peer's published public key alone"
    );
}

#[tokio::test]
async fn a_token_naming_a_key_daemon_b_has_never_seen_is_refused() {
    // Given a daemon that signed a token but never published its key
    let fleet = a_common_key_directory();
    let stranger = a_daemon();
    let token = stranger.signer().mint_access(&an_operator());

    // And daemon B, which has therefore never seen that key
    let daemon_b = a_daemon_that_has_published_its_key(&fleet).await;

    // When B verifies the token
    let refusal = the_verifier_for(&daemon_b, &fleet).verify(&token).await;

    // Then it is refused by name — an unknown signer is not a signer to trust
    assert_eq!(
        refusal.map(|_| ()),
        Err(SessionTokenError::UnknownKeyId(stranger.key_id())),
        "an unpublished key must be refused, never accepted on a fallback"
    );
}

#[tokio::test]
async fn a_daemon_admits_the_token_it_minted_itself_before_publishing_anything() {
    // Given a daemon that has joined no fleet and published no key
    let fleet = a_common_key_directory();
    let daemon = a_daemon();

    // When it verifies a token it minted for its own operator
    let token = daemon.signer().mint_access(&an_operator());
    let claims = the_verifier_for(&daemon, &fleet).verify(&token).await;

    // Then it admits it — a desktop install has no peers and must still authenticate its own user
    assert_eq!(claims.map(|claims| claims.login), Ok(THE_LOGIN.to_string()));
}

/// A daemon with a keypair of its own, persisted where a restart would find it.
fn a_daemon() -> DaemonSigningKey {
    let dir = tempfile::tempdir().expect("a temporary directory");
    DaemonSigningKey::load_or_generate(&dir.path().join("signing_key.pem"))
        .expect("a daemon generates a keypair on first use")
}

async fn a_daemon_that_has_published_its_key(
    fleet: &Arc<InMemoryKeyDirectory>,
) -> DaemonSigningKey {
    let daemon = a_daemon();
    fleet
        .publish(&daemon.key_id(), &daemon.verifying_key())
        .await
        .expect("publishing a public key to the common room succeeds");
    daemon
}

fn the_verifier_for(
    daemon: &DaemonSigningKey,
    fleet: &Arc<InMemoryKeyDirectory>,
) -> DirectorySessionTokenVerifier {
    DirectorySessionTokenVerifier::new(daemon, fleet.clone())
}

fn an_operator() -> GitHubUser {
    GitHubUser {
        id: 1,
        login: THE_LOGIN.to_string(),
        avatar_url: String::new(),
        name: THE_LOGIN.to_string(),
    }
}

fn a_common_key_directory() -> Arc<InMemoryKeyDirectory> {
    Arc::new(InMemoryKeyDirectory::default())
}

/// The fleet's key distribution, as a fake rather than a mock.
///
/// A real [`KeyDirectory`] is the LiveKit common room: daemons announce their public halves onto it
/// and read each other's back. Every daemon in a test shares one of these, so "B has seen A's key"
/// is expressed by A having published — which is the actual precondition, not a stubbed answer.
#[derive(Default)]
struct InMemoryKeyDirectory {
    published: Mutex<HashMap<KeyId, VerifyingKey>>,
}

#[async_trait]
impl KeyDirectory for InMemoryKeyDirectory {
    async fn publish(&self, key_id: &KeyId, public_key: &VerifyingKey) -> anyhow::Result<()> {
        self.published
            .lock()
            .expect("the directory is not poisoned")
            .insert(key_id.clone(), *public_key);
        Ok(())
    }

    async fn public_key_for(&self, key_id: &KeyId) -> anyhow::Result<Option<VerifyingKey>> {
        Ok(self
            .published
            .lock()
            .expect("the directory is not poisoned")
            .get(key_id)
            .copied())
    }
}

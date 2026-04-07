//! Telegram user ↔ GitHub login binding for OAuth and workflow OS-user resolution.
//!
//! Acceptance tests in `tests/telegram_github_link.rs` exercise this module.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::config::DaemonConfig;
use tddy_github::GitHubOAuthProvider;

type HmacSha256 = Hmac<Sha256>;

const OAUTH_STATE_PREFIX: &str = "v1.";
/// `[version: u8][telegram_user_id: u8 LE x8]`
const OAUTH_PAYLOAD_LEN: usize = 9;
const OAUTH_TAG_LEN: usize = 32;

// ---------------------------------------------------------------------------
// OAuth state (cryptographic binding of browser round-trip to `telegram_user_id`)
// ---------------------------------------------------------------------------

/// Errors from validating Telegram-scoped OAuth `state` values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TelegramOAuthStateError {
    InvalidSignature,
    Malformed,
}

impl std::fmt::Display for TelegramOAuthStateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSignature => write!(f, "telegram OAuth state: invalid signature"),
            Self::Malformed => write!(f, "telegram OAuth state: malformed payload"),
        }
    }
}

impl std::error::Error for TelegramOAuthStateError {}

/// Signs OAuth `state` so the callback can be attributed to a Telegram user (see PRD).
pub struct TelegramOAuthStateSigner {
    secret: Vec<u8>,
}

impl TelegramOAuthStateSigner {
    pub fn new(secret: &[u8]) -> Self {
        log::debug!(
            target: "tddy_daemon::telegram_github_link",
            "TelegramOAuthStateSigner::new: secret_len={}",
            secret.len()
        );
        Self {
            secret: secret.to_vec(),
        }
    }

    fn sign_payload(&self, payload: &[u8; OAUTH_PAYLOAD_LEN]) -> [u8; OAUTH_TAG_LEN] {
        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .expect("HMAC-SHA256 accepts arbitrary key lengths");
        mac.update(payload);
        let mut tag = [0u8; OAUTH_TAG_LEN];
        tag.copy_from_slice(&mac.finalize().into_bytes());
        tag
    }

    /// Encode `telegram_user_id` into an opaque `state` string for the GitHub authorize URL.
    pub fn encode_telegram_user(
        &self,
        telegram_user_id: u64,
    ) -> Result<String, TelegramOAuthStateError> {
        let mut payload = [0u8; OAUTH_PAYLOAD_LEN];
        payload[0] = 1;
        payload[1..].copy_from_slice(&telegram_user_id.to_le_bytes());
        let tag = self.sign_payload(&payload);
        let mut raw = Vec::with_capacity(OAUTH_PAYLOAD_LEN + OAUTH_TAG_LEN);
        raw.extend_from_slice(&payload);
        raw.extend_from_slice(&tag);
        let b64 = URL_SAFE_NO_PAD.encode(&raw);
        let state = format!("{OAUTH_STATE_PREFIX}{b64}");
        log::debug!(
            target: "tddy_daemon::telegram_github_link",
            "encode_telegram_user: uid={} state_len={}",
            telegram_user_id,
            state.len()
        );
        Ok(state)
    }

    /// Verify `state` and return the bound Telegram user id.
    pub fn verify_and_extract_telegram_user(
        &self,
        state: &str,
    ) -> Result<u64, TelegramOAuthStateError> {
        let rest = state
            .strip_prefix(OAUTH_STATE_PREFIX)
            .ok_or(TelegramOAuthStateError::Malformed)?;
        let raw = URL_SAFE_NO_PAD
            .decode(rest.as_bytes())
            .map_err(|_| TelegramOAuthStateError::Malformed)?;
        if raw.len() != OAUTH_PAYLOAD_LEN + OAUTH_TAG_LEN {
            return Err(TelegramOAuthStateError::Malformed);
        }
        let payload: &[u8; OAUTH_PAYLOAD_LEN] = raw[..OAUTH_PAYLOAD_LEN]
            .try_into()
            .map_err(|_| TelegramOAuthStateError::Malformed)?;
        let tag: &[u8; OAUTH_TAG_LEN] = raw[OAUTH_PAYLOAD_LEN..]
            .try_into()
            .map_err(|_| TelegramOAuthStateError::Malformed)?;
        if payload[0] != 1 {
            return Err(TelegramOAuthStateError::Malformed);
        }
        let expected = self.sign_payload(payload);
        if !bool::from(expected.ct_eq(tag)) {
            log::debug!(
                target: "tddy_daemon::telegram_github_link",
                "verify_and_extract_telegram_user: HMAC mismatch"
            );
            return Err(TelegramOAuthStateError::InvalidSignature);
        }
        let uid = u64::from_le_bytes(payload[1..].try_into().unwrap());
        log::debug!(
            target: "tddy_daemon::telegram_github_link",
            "verify_and_extract_telegram_user: ok uid={}",
            uid
        );
        Ok(uid)
    }
}

// ---------------------------------------------------------------------------
// Durable mapping: telegram_user_id → github_login
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TelegramGithubMappingFile {
    #[serde(default = "mapping_file_version")]
    version: u32,
    #[serde(default)]
    mappings: HashMap<String, String>,
}

fn mapping_file_version() -> u32 {
    1
}

/// Persistent store for Telegram → GitHub login associations (daemon data directory).
pub struct TelegramGithubMappingStore {
    path: PathBuf,
}

impl TelegramGithubMappingStore {
    pub fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        log::debug!(
            target: "tddy_daemon::telegram_github_link",
            "TelegramGithubMappingStore::open path={}",
            path.display()
        );
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn load(&self) -> anyhow::Result<TelegramGithubMappingFile> {
        if !self.path.exists() {
            return Ok(TelegramGithubMappingFile::default());
        }
        let bytes = std::fs::read(&self.path)?;
        let parsed: TelegramGithubMappingFile = serde_json::from_slice(&bytes)?;
        Ok(parsed)
    }

    fn save_atomic(&self, file: &TelegramGithubMappingFile) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(file)?;
        let tmp = self
            .path
            .with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&tmp, &json)?;
        std::fs::rename(&tmp, &self.path)?;
        log::debug!(
            target: "tddy_daemon::telegram_github_link",
            "TelegramGithubMappingStore: wrote {} bytes atomically to {}",
            json.len(),
            self.path.display()
        );
        Ok(())
    }

    pub fn put(&mut self, telegram_user_id: u64, github_login: &str) -> anyhow::Result<()> {
        log::info!(
            target: "tddy_daemon::telegram_github_link",
            "TelegramGithubMappingStore::put telegram_user_id={} github_login_len={}",
            telegram_user_id,
            github_login.len()
        );
        let mut file = self.load()?;
        file.mappings
            .insert(telegram_user_id.to_string(), github_login.to_string());
        self.save_atomic(&file)?;
        Ok(())
    }

    pub fn get_github_login(&self, telegram_user_id: u64) -> Option<String> {
        log::debug!(
            target: "tddy_daemon::telegram_github_link",
            "TelegramGithubMappingStore::get_github_lookup telegram_user_id={}",
            telegram_user_id
        );
        let file = self.load().ok()?;
        file.mappings.get(&telegram_user_id.to_string()).cloned()
    }
}

/// Resolve the OS user for Telegram-driven workflow spawn using `users:` mapping in config.
pub fn resolved_os_user_for_telegram_workflow(
    config: &DaemonConfig,
    store: &TelegramGithubMappingStore,
    telegram_user_id: u64,
) -> Option<String> {
    log::debug!(
        target: "tddy_daemon::telegram_github_link",
        "resolved_os_user_for_telegram_workflow: telegram_user_id={}",
        telegram_user_id
    );
    let login = store.get_github_login(telegram_user_id)?;
    let os = config.os_user_for_github(&login).map(str::to_string);
    log::debug!(
        target: "tddy_daemon::telegram_github_link",
        "resolved_os_user_for_telegram_workflow: github_login={} os_user={:?}",
        login,
        os
    );
    os
}

/// Complete linking using a stub authorization code (same mapping path as real OAuth).
pub fn complete_telegram_link_via_stub_exchange(
    provider: &tddy_github::StubGitHubProvider,
    code: &str,
    telegram_user_id: u64,
    store: &mut TelegramGithubMappingStore,
) -> anyhow::Result<String> {
    log::info!(
        target: "tddy_daemon::telegram_github_link",
        "complete_telegram_link_via_stub_exchange: telegram_user_id={} code_len={}",
        telegram_user_id,
        code.len()
    );
    let (_url, state) = provider.authorize_url();
    log::debug!(
        target: "tddy_daemon::telegram_github_link",
        "stub OAuth: authorize state registered (state_len={})",
        state.len()
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let exchanged = runtime.block_on(provider.exchange_code(code, &state));
    let (_token, user) = exchanged.map_err(|e| anyhow::anyhow!("stub GitHub exchange: {e}"))?;
    let login = user.login.clone();
    store.put(telegram_user_id, &login)?;
    log::info!(
        target: "tddy_daemon::telegram_github_link",
        "complete_telegram_link_via_stub_exchange: stored mapping telegram_user_id={} github_login={}",
        telegram_user_id,
        login
    );
    Ok(login)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_mapping_store_put_get_roundtrip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("m.json");
        let mut store = TelegramGithubMappingStore::open(&path).expect("open");
        store.put(1, "u").expect("put must persist mapping");
        assert_eq!(store.get_github_login(1).as_deref(), Some("u"));
    }

    #[test]
    fn unit_oauth_encode_returns_opaque_state() {
        let s = TelegramOAuthStateSigner::new(&[0u8; 32]);
        assert!(
            s.encode_telegram_user(42).expect("encode").len() >= 8,
            "encoded state should be non-trivial"
        );
    }

    #[test]
    fn unit_stub_exchange_updates_mapping_store() {
        let stub = tddy_github::StubGitHubProvider::new("https://github.com", "cid");
        stub.register_code(
            "c1",
            tddy_github::GitHubUser {
                id: 1,
                login: "login-a".to_string(),
                avatar_url: "https://a".to_string(),
                name: "A".to_string(),
            },
        );
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut store = TelegramGithubMappingStore::open(tmp.path().join("x.json")).expect("open");
        let login =
            complete_telegram_link_via_stub_exchange(&stub, "c1", 9, &mut store).expect("link");
        assert_eq!(login, "login-a");
        assert_eq!(store.get_github_login(9).as_deref(), Some("login-a"));
    }
}

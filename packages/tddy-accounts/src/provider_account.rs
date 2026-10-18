//! Model-provider accounts: an API key for Meta's Model API, Fireworks, Baseten, Modal or a
//! compatible endpoint, held in the vault beside the GitHub accounts.
//!
//! A provider account is an ordinary [`CredentialRecord`] — `provider` is the preset id, `secret`
//! is the key, and the endpoints it serves live in `metadata` — so it is sealed, listed, relabelled,
//! removed and synced exactly like every other account. This module is the one place that knows how
//! a record spells a provider account: [`ProviderAccount::to_record`] and
//! [`ProviderAccount::from_record`] are inverse, and nothing else reads those metadata keys.
//!
//! A preset fixes, per wire protocol, the root a client is pointed at. Which harness can run on an
//! account follows from the protocols it serves, so an account never records a harness.

use async_trait::async_trait;
use tddy_credentials::{AccountId, CredentialRecord, SecretString};

use crate::store::AccountsError;

/// A protocol a provider endpoint speaks, named as `AccountSummary.wire_protocols` spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WireProtocol {
    /// Anthropic Messages (`POST <root>/v1/messages`) — what Claude Code speaks.
    AnthropicMessages,
    /// OpenAI Responses (`POST <root>/responses`) — what Codex speaks.
    OpenAiResponses,
    /// OpenAI Chat Completions (`POST <root>/chat/completions`).
    OpenAiChat,
}

impl WireProtocol {
    /// The wire spelling: `"anthropic-messages"`, `"openai-responses"`, `"openai-chat"`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        // TODO(accounts): implement
        todo!()
    }
}

/// Where one preset's endpoint for a protocol comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetEndpoint {
    /// The provider does not serve this protocol.
    NotServed,
    /// A fixed, documented root the operator cannot override.
    Fixed(&'static str),
    /// The operator supplies the root when connecting (a per-deployment URL).
    OperatorSupplied,
}

/// One connectable provider, and the roots it serves per protocol.
#[derive(Debug, PartialEq, Eq)]
pub struct ProviderPreset {
    /// The vault `provider` id and the wire `preset` — `"meta"`, `"fireworks"`, …
    pub id: &'static str,
    /// What the Accounts screen calls it.
    pub label: &'static str,
    pub anthropic_messages: PresetEndpoint,
    pub openai_responses: PresetEndpoint,
    pub openai_chat: PresetEndpoint,
}

/// The roots one account's endpoint serves. A protocol it does not serve is `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderEndpoints {
    /// The Anthropic Messages root; the client appends `/v1/messages`.
    pub anthropic_messages: Option<String>,
    /// The OpenAI root `/responses` is appended to.
    pub openai_responses: Option<String>,
    /// The OpenAI root `/chat/completions` and `/models` are appended to.
    pub openai_chat: Option<String>,
}

/// Why a connect request does not describe a usable account. Each is the operator's to fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderAccountError {
    /// No preset has this id.
    UnknownPreset(String),
    /// The preset's endpoint is operator-supplied and the request named none.
    MissingBaseUrl {
        preset: &'static str,
        protocol: WireProtocol,
    },
    /// The preset's endpoint is fixed and the request tried to replace it.
    FixedBaseUrl {
        preset: &'static str,
        protocol: WireProtocol,
    },
    /// A supplied root is not an `http(s)` URL with a host, or carries userinfo.
    InvalidBaseUrl(String),
    /// An API key is required and the request carried none.
    MissingApiKey,
}

/// Every preset an account can be connected with, in the order the Accounts screen offers them.
pub fn presets() -> &'static [ProviderPreset] {
    // TODO(accounts): implement
    todo!()
}

/// The preset with this id.
#[must_use]
pub fn preset(id: &str) -> Option<&'static ProviderPreset> {
    let _ = id;
    // TODO(accounts): implement
    todo!()
}

/// The endpoints an account connected with `preset` serves, given the roots the operator supplied.
///
/// A fixed root is taken from the preset and may not be supplied; an operator-supplied root must
/// be. One `openai_base_url` serves both OpenAI protocols where the preset serves both.
pub fn endpoints_for(
    preset: &ProviderPreset,
    anthropic_base_url: Option<&str>,
    openai_base_url: Option<&str>,
) -> Result<ProviderEndpoints, ProviderAccountError> {
    let _ = (preset, anthropic_base_url, openai_base_url);
    // TODO(accounts): implement
    todo!()
}

/// A model-provider account, decoded from the vault record that holds it.
#[derive(Debug, Clone)]
pub struct ProviderAccount {
    pub account: AccountId,
    pub preset: &'static ProviderPreset,
    pub label: String,
    pub endpoints: ProviderEndpoints,
    pub api_key: SecretString,
}

impl ProviderAccount {
    /// The vault record that holds this account, written at `now` (seconds since the epoch).
    #[must_use]
    pub fn to_record(&self, now: u64) -> CredentialRecord {
        let _ = now;
        // TODO(accounts): implement
        todo!()
    }

    /// The provider account `record` holds, or `None` when it holds some other kind of account
    /// (a GitHub login, a screen-sharing password).
    #[must_use]
    pub fn from_record(record: &CredentialRecord) -> Option<ProviderAccount> {
        let _ = record;
        // TODO(accounts): implement
        todo!()
    }

    /// The protocols this account's endpoint serves, in [`WireProtocol`] order.
    #[must_use]
    pub fn wire_protocols(&self) -> Vec<WireProtocol> {
        // TODO(accounts): implement
        todo!()
    }
}

/// Asks a provider endpoint which models it serves, with one account's key.
///
/// A port, because the HTTP client is the daemon's: this crate stays free of a network stack. The
/// same call is the connect-time check that a key works.
#[async_trait]
pub trait ProviderModelProbe: Send + Sync {
    /// The model ids the endpoint lists, or the provider's own reason it refused.
    async fn list_models(
        &self,
        endpoints: &ProviderEndpoints,
        api_key: &SecretString,
    ) -> Result<Vec<String>, String>;
}

/// Where provider accounts are written and read, per signed-in session.
pub trait ProviderAccountStore: Send + Sync {
    /// Store a newly connected account in the session owner's vault. The record as written.
    fn put_provider_account(
        &self,
        session_token: &str,
        account: &ProviderAccount,
    ) -> Result<CredentialRecord, AccountsError>;

    /// The provider account `account` names in the session owner's vault.
    fn provider_account(
        &self,
        session_token: &str,
        account: &AccountId,
    ) -> Result<ProviderAccount, AccountsError>;
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pretty_assertions::assert_eq;
    use tddy_credentials::{ProviderId, FIRST_VERSION};

    use super::*;

    fn the_preset(id: &str) -> &'static ProviderPreset {
        preset(id).unwrap_or_else(|| panic!("no preset {id}"))
    }

    fn a_meta_account() -> ProviderAccount {
        let meta = the_preset("meta");
        ProviderAccount {
            account: AccountId::new("meta-hackathon"),
            preset: meta,
            label: "Meta hackathon".to_string(),
            endpoints: endpoints_for(meta, None, None).expect("meta endpoints"),
            api_key: SecretString::new("a-meta-key".to_string()),
        }
    }

    #[test]
    fn meta_preset_serves_messages_responses_and_chat_at_its_documented_roots() {
        // When
        let endpoints = endpoints_for(the_preset("meta"), None, None).expect("fixed endpoints");

        // Then
        assert_eq!(
            endpoints,
            ProviderEndpoints {
                anthropic_messages: Some("https://api.meta.ai".to_string()),
                openai_responses: Some("https://api.meta.ai/v1".to_string()),
                openai_chat: Some("https://api.meta.ai/v1".to_string()),
            }
        );
    }

    #[test]
    fn fireworks_serves_messages_at_the_inference_root_and_chat_under_its_v1() {
        // When
        let endpoints =
            endpoints_for(the_preset("fireworks"), None, None).expect("fixed endpoints");

        // Then
        assert_eq!(
            endpoints,
            ProviderEndpoints {
                anthropic_messages: Some("https://api.fireworks.ai/inference".to_string()),
                openai_responses: None,
                openai_chat: Some("https://api.fireworks.ai/inference/v1".to_string()),
            }
        );
    }

    #[test]
    fn the_presets_are_offered_in_a_fixed_order() {
        // When
        let ids: Vec<&str> = presets().iter().map(|preset| preset.id).collect();

        // Then
        assert_eq!(
            ids,
            vec![
                "meta",
                "fireworks",
                "baseten",
                "modal",
                "openai-compatible",
                "anthropic-compatible"
            ]
        );
    }

    #[test]
    fn an_operator_supplied_root_is_required_and_used_for_both_openai_protocols() {
        // Given
        let generic = the_preset("openai-compatible");

        // When
        let missing = endpoints_for(generic, None, None);
        let supplied = endpoints_for(generic, None, Some("https://llm.example.com/v1"));

        // Then
        assert_eq!(
            missing,
            Err(ProviderAccountError::MissingBaseUrl {
                preset: "openai-compatible",
                protocol: WireProtocol::OpenAiChat,
            })
        );
        let supplied = supplied.expect("endpoints");
        assert_eq!(
            supplied.openai_chat.as_deref(),
            Some("https://llm.example.com/v1")
        );
        assert_eq!(
            supplied.openai_responses.as_deref(),
            Some("https://llm.example.com/v1")
        );
        assert_eq!(supplied.anthropic_messages, None);
    }

    #[test]
    fn a_fixed_root_cannot_be_replaced() {
        // When
        let overridden = endpoints_for(the_preset("meta"), None, Some("https://evil.example/v1"));

        // Then
        assert!(matches!(
            overridden,
            Err(ProviderAccountError::FixedBaseUrl { preset: "meta", .. })
        ));
    }

    #[test]
    fn a_root_with_userinfo_or_a_non_http_scheme_is_refused() {
        // Given
        let generic = the_preset("openai-compatible");

        // Then
        assert!(matches!(
            endpoints_for(generic, None, Some("https://user:pw@llm.example.com/v1")),
            Err(ProviderAccountError::InvalidBaseUrl(_))
        ));
        assert!(matches!(
            endpoints_for(generic, None, Some("file:///etc/passwd")),
            Err(ProviderAccountError::InvalidBaseUrl(_))
        ));
    }

    #[test]
    fn a_provider_account_round_trips_through_a_credential_record() {
        // Given
        let account = a_meta_account();

        // When
        let record = account.to_record(1_760_000_000);
        let decoded = ProviderAccount::from_record(&record).expect("a provider account");

        // Then
        assert_eq!(record.provider, ProviderId::new("meta"));
        assert_eq!(record.account, AccountId::new("meta-hackathon"));
        assert_eq!(record.label, "Meta hackathon");
        assert_eq!(record.secret.expose(), "a-meta-key");
        assert_eq!(record.updated_at, 1_760_000_000);
        assert_eq!(decoded.preset.id, "meta");
        assert_eq!(decoded.endpoints, account.endpoints);
        assert_eq!(decoded.api_key.expose(), "a-meta-key");
    }

    #[test]
    fn a_github_record_is_not_a_provider_account() {
        // Given
        let github = CredentialRecord {
            provider: ProviderId::new("github"),
            account: AccountId::new("github-1024"),
            label: "Ada".to_string(),
            secret: SecretString::new("gho_token".to_string()),
            metadata: BTreeMap::from([("subject".to_string(), "ada".to_string())]),
            updated_at: 1,
            version: FIRST_VERSION,
        };

        // Then
        assert!(ProviderAccount::from_record(&github).is_none());
    }

    #[test]
    fn the_wire_protocols_follow_the_endpoints_served() {
        // Given
        let fireworks = the_preset("fireworks");
        let account = ProviderAccount {
            endpoints: endpoints_for(fireworks, None, None).expect("endpoints"),
            preset: fireworks,
            ..a_meta_account()
        };

        // Then
        assert_eq!(
            account.wire_protocols(),
            vec![WireProtocol::AnthropicMessages, WireProtocol::OpenAiChat]
        );
        assert_eq!(
            account
                .wire_protocols()
                .into_iter()
                .map(WireProtocol::as_str)
                .collect::<Vec<_>>(),
            vec!["anthropic-messages", "openai-chat"]
        );
    }
}

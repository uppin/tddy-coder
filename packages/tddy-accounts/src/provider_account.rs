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

use std::collections::BTreeMap;

use async_trait::async_trait;
use tddy_credentials::{AccountId, CredentialRecord, ProviderId, SecretString, FIRST_VERSION};

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
        match self {
            Self::AnthropicMessages => "anthropic-messages",
            Self::OpenAiResponses => "openai-responses",
            Self::OpenAiChat => "openai-chat",
        }
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

impl PresetEndpoint {
    /// Whether the provider serves this protocol at all.
    #[must_use]
    pub fn is_served(self) -> bool {
        self != Self::NotServed
    }
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
    /// The request supplied a root for a protocol the preset does not serve. Refused rather than
    /// dropped: a URL the operator typed is never silently ignored.
    UnservedBaseUrl {
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
    use PresetEndpoint::{Fixed, NotServed, OperatorSupplied};
    static PRESETS: [ProviderPreset; 6] = [
        ProviderPreset {
            id: "meta",
            label: "Meta",
            anthropic_messages: Fixed("https://api.meta.ai"),
            openai_responses: Fixed("https://api.meta.ai/v1"),
            openai_chat: Fixed("https://api.meta.ai/v1"),
        },
        ProviderPreset {
            id: "fireworks",
            label: "Fireworks",
            anthropic_messages: Fixed("https://api.fireworks.ai/inference"),
            openai_responses: NotServed,
            openai_chat: Fixed("https://api.fireworks.ai/inference/v1"),
        },
        ProviderPreset {
            id: "baseten",
            label: "Baseten",
            anthropic_messages: NotServed,
            openai_responses: NotServed,
            openai_chat: Fixed("https://inference.baseten.co/v1"),
        },
        ProviderPreset {
            id: "modal",
            label: "Modal",
            anthropic_messages: NotServed,
            openai_responses: NotServed,
            openai_chat: OperatorSupplied,
        },
        ProviderPreset {
            id: "openai-compatible",
            label: "OpenAI-compatible",
            anthropic_messages: NotServed,
            openai_responses: OperatorSupplied,
            openai_chat: OperatorSupplied,
        },
        ProviderPreset {
            id: "anthropic-compatible",
            label: "Anthropic-compatible",
            anthropic_messages: OperatorSupplied,
            openai_responses: NotServed,
            openai_chat: NotServed,
        },
    ];
    &PRESETS
}

/// The preset with this id.
#[must_use]
pub fn preset(id: &str) -> Option<&'static ProviderPreset> {
    presets().iter().find(|preset| preset.id == id)
}

/// The endpoints an account connected with `preset` serves, given the roots the operator supplied.
///
/// A fixed root is taken from the preset and may not be supplied; an operator-supplied root must
/// be; a root for a protocol the preset does not serve is refused. One `openai_base_url` serves
/// both OpenAI protocols where the preset serves both, so it is unserved only when neither is.
pub fn endpoints_for(
    preset: &ProviderPreset,
    anthropic_base_url: Option<&str>,
    openai_base_url: Option<&str>,
) -> Result<ProviderEndpoints, ProviderAccountError> {
    let unserved = |protocol| ProviderAccountError::UnservedBaseUrl {
        preset: preset.id,
        protocol,
    };
    if anthropic_base_url.is_some() && !preset.anthropic_messages.is_served() {
        return Err(unserved(WireProtocol::AnthropicMessages));
    }
    if openai_base_url.is_some()
        && !preset.openai_chat.is_served()
        && !preset.openai_responses.is_served()
    {
        return Err(unserved(WireProtocol::OpenAiChat));
    }
    // Chat is resolved before Responses: it is the protocol every OpenAI-speaking preset serves,
    // so it is the one a refusal names.
    let anthropic_messages = resolve_root(
        preset.id,
        WireProtocol::AnthropicMessages,
        preset.anthropic_messages,
        anthropic_base_url,
    )?;
    let openai_chat = resolve_root(
        preset.id,
        WireProtocol::OpenAiChat,
        preset.openai_chat,
        openai_base_url,
    )?;
    let openai_responses = resolve_root(
        preset.id,
        WireProtocol::OpenAiResponses,
        preset.openai_responses,
        openai_base_url,
    )?;
    Ok(ProviderEndpoints {
        anthropic_messages,
        openai_responses,
        openai_chat,
    })
}

/// The root one protocol is served at: the preset's fixed one, or the operator's, validated.
fn resolve_root(
    preset: &'static str,
    protocol: WireProtocol,
    source: PresetEndpoint,
    supplied: Option<&str>,
) -> Result<Option<String>, ProviderAccountError> {
    match (source, supplied) {
        (PresetEndpoint::NotServed, _) => Ok(None),
        (PresetEndpoint::Fixed(root), None) => Ok(Some(root.to_string())),
        (PresetEndpoint::Fixed(_), Some(_)) => {
            Err(ProviderAccountError::FixedBaseUrl { preset, protocol })
        }
        (PresetEndpoint::OperatorSupplied, None) => {
            Err(ProviderAccountError::MissingBaseUrl { preset, protocol })
        }
        (PresetEndpoint::OperatorSupplied, Some(root)) => valid_root(root).map(Some),
    }
}

/// `root` without a trailing slash, when it is an `http(s)` URL with a host and nothing that would
/// smuggle a credential or reshape the request (userinfo, query, fragment, whitespace).
fn valid_root(root: &str) -> Result<String, ProviderAccountError> {
    let invalid = || ProviderAccountError::InvalidBaseUrl(root.to_string());
    let rest = root
        .strip_prefix("https://")
        .or_else(|| root.strip_prefix("http://"))
        .ok_or_else(invalid)?;
    let authority = rest.split('/').next().unwrap_or_default();
    let unusable = authority.is_empty()
        || authority.contains('@')
        || root.contains(['?', '#'])
        || root.chars().any(char::is_whitespace);
    if unusable {
        return Err(invalid());
    }
    Ok(root.trim_end_matches('/').to_string())
}

const META_PRESET: &str = "preset";
const META_PROBED_AT: &str = "probed_at";

fn endpoint_key(protocol: WireProtocol) -> String {
    format!("endpoint.{}", protocol.as_str())
}

/// The id a new account connected with `preset` and `label` asks for: the preset and the label's
/// slug (`prod` on Fireworks is `fireworks-prod`), the slug alone when it already starts with the
/// preset id (`Meta hackathon` is `meta-hackathon`, not `meta-meta-hackathon`), or the bare preset
/// id when the label has no letters or digits. The store numbers it when that id is taken — see
/// [`unclaimed_account_id`].
#[must_use]
pub fn account_id_for(preset: &ProviderPreset, label: &str) -> AccountId {
    let lowered: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = lowered
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let names_its_preset = slug == preset.id || slug.starts_with(&format!("{}-", preset.id));
    if slug.is_empty() {
        AccountId::new(preset.id)
    } else if names_its_preset {
        AccountId::new(slug)
    } else {
        AccountId::new(format!("{}-{slug}", preset.id))
    }
}

/// `wanted`, or the first of `<wanted>-2`, `<wanted>-3`, … that no record in `held` uses.
#[must_use]
pub fn unclaimed_account_id(wanted: &AccountId, held: &[CredentialRecord]) -> AccountId {
    let taken = |id: &str| held.iter().any(|record| record.account.as_str() == id);
    let mut candidate = wanted.as_str().to_string();
    let mut n = 2;
    while taken(&candidate) {
        candidate = format!("{}-{n}", wanted.as_str());
        n += 1;
    }
    AccountId::new(candidate)
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
    ///
    /// `now` is also when the key was last shown to work, since an account is only ever written
    /// straight after its endpoint listed models.
    #[must_use]
    pub fn to_record(&self, now: u64) -> CredentialRecord {
        let mut metadata = BTreeMap::from([
            (META_PRESET.to_string(), self.preset.id.to_string()),
            (META_PROBED_AT.to_string(), now.to_string()),
        ]);
        for (protocol, root) in self.served() {
            metadata.insert(endpoint_key(protocol), root.to_string());
        }
        CredentialRecord {
            provider: ProviderId::new(self.preset.id),
            account: self.account.clone(),
            label: self.label.clone(),
            secret: self.api_key.clone(),
            metadata,
            updated_at: now,
            version: FIRST_VERSION,
        }
    }

    /// The provider account `record` holds, or `None` when it holds some other kind of account
    /// (a GitHub login, a screen-sharing password).
    #[must_use]
    pub fn from_record(record: &CredentialRecord) -> Option<ProviderAccount> {
        let preset = preset(record.metadata.get(META_PRESET)?)?;
        if record.provider.as_str() != preset.id {
            return None;
        }
        let root = |protocol| record.metadata.get(&endpoint_key(protocol)).cloned();
        Some(ProviderAccount {
            account: record.account.clone(),
            preset,
            label: record.label.clone(),
            endpoints: ProviderEndpoints {
                anthropic_messages: root(WireProtocol::AnthropicMessages),
                openai_responses: root(WireProtocol::OpenAiResponses),
                openai_chat: root(WireProtocol::OpenAiChat),
            },
            api_key: record.secret.clone(),
        })
    }

    /// The protocols this account's endpoint serves, in [`WireProtocol`] order.
    #[must_use]
    pub fn wire_protocols(&self) -> Vec<WireProtocol> {
        self.served()
            .into_iter()
            .map(|(protocol, _)| protocol)
            .collect()
    }

    fn served(&self) -> Vec<(WireProtocol, &str)> {
        [
            (
                WireProtocol::AnthropicMessages,
                &self.endpoints.anthropic_messages,
            ),
            (
                WireProtocol::OpenAiResponses,
                &self.endpoints.openai_responses,
            ),
            (WireProtocol::OpenAiChat, &self.endpoints.openai_chat),
        ]
        .into_iter()
        .filter_map(|(protocol, root)| root.as_deref().map(|root| (protocol, root)))
        .collect()
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
    /// Store a newly connected account in the session owner's vault, never over a held record.
    ///
    /// The account is stored under `account.account`, or — when that id is already held — under
    /// [`unclaimed_account_id`] of it, decided atomically with the write: two connects asking for
    /// the same id at once get two ids, and neither replaces the other. The record as written,
    /// carrying the id it was given.
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
    fn a_root_for_a_protocol_the_preset_does_not_serve_is_refused() {
        // When
        let anthropic_for_modal = endpoints_for(
            the_preset("modal"),
            Some("https://llm.example.com"),
            Some("https://modal.example/v1"),
        );
        let openai_for_anthropic_compatible = endpoints_for(
            the_preset("anthropic-compatible"),
            Some("https://llm.example.com"),
            Some("https://llm.example.com/v1"),
        );
        let anthropic_for_baseten =
            endpoints_for(the_preset("baseten"), Some("https://llm.example.com"), None);

        // Then
        assert_eq!(
            anthropic_for_modal,
            Err(ProviderAccountError::UnservedBaseUrl {
                preset: "modal",
                protocol: WireProtocol::AnthropicMessages,
            })
        );
        assert_eq!(
            openai_for_anthropic_compatible,
            Err(ProviderAccountError::UnservedBaseUrl {
                preset: "anthropic-compatible",
                protocol: WireProtocol::OpenAiChat,
            })
        );
        assert!(matches!(
            anthropic_for_baseten,
            Err(ProviderAccountError::UnservedBaseUrl {
                preset: "baseten",
                ..
            })
        ));
    }

    #[test]
    fn modal_takes_its_openai_root_although_it_serves_only_chat() {
        // When
        let endpoints = endpoints_for(the_preset("modal"), None, Some("https://modal.example/v1/"))
            .expect("endpoints");

        // Then
        assert_eq!(
            endpoints,
            ProviderEndpoints {
                anthropic_messages: None,
                openai_responses: None,
                openai_chat: Some("https://modal.example/v1".to_string()),
            }
        );
    }

    #[test]
    fn a_new_account_asks_for_the_preset_and_its_labels_slug() {
        // Then
        assert_eq!(
            account_id_for(the_preset("meta"), "Meta  Hackathon (2026)!"),
            AccountId::new("meta-hackathon-2026")
        );
        assert_eq!(
            account_id_for(the_preset("fireworks"), "Prod"),
            AccountId::new("fireworks-prod")
        );
        assert_eq!(
            account_id_for(the_preset("meta"), "Metabase"),
            AccountId::new("meta-metabase")
        );
        assert_eq!(
            account_id_for(the_preset("fireworks"), "  ***  "),
            AccountId::new("fireworks")
        );
    }

    #[test]
    fn a_held_id_is_numbered_from_two() {
        // Given
        let held = vec![
            a_meta_account().to_record(1),
            ProviderAccount {
                account: AccountId::new("meta-hackathon-2"),
                ..a_meta_account()
            }
            .to_record(1),
        ];

        // Then
        assert_eq!(
            unclaimed_account_id(&AccountId::new("meta-hackathon"), &held),
            AccountId::new("meta-hackathon-3")
        );
        assert_eq!(
            unclaimed_account_id(&AccountId::new("meta-other"), &held),
            AccountId::new("meta-other")
        );
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

use crate::linking::META_SUBJECT;
use crate::provider_account::{
    account_id_for, defaults_of, endpoints_for, preset, ProviderAccount, ProviderAccountError,
};
use crate::store::AccountsError;
use crate::sync_status::SyncStatusSource;
use tddy_credential_sync::AccountSyncSummary;
use tddy_credentials::{CredentialRecord, SecretString};
use tddy_rpc::Status;
use tddy_service::proto::accounts::{
    AccountSummary, ConnectProviderAccountRequest, ProviderAccounts, SyncStatus,
};

/// Group records by provider in the order the store returned them — the store already orders by
/// `(provider, account)`, so this never re-sorts.
pub(super) fn grouped_by_provider(
    records: &[CredentialRecord],
    sync_status: Option<&dyn SyncStatusSource>,
) -> Vec<ProviderAccounts> {
    let mut groups: Vec<ProviderAccounts> = Vec::new();
    for record in records {
        let provider = record.provider.as_str();
        let summary = summary_of(record, sync_status);
        match groups.iter_mut().find(|group| group.provider == provider) {
            Some(group) => group.accounts.push(summary),
            None => groups.push(ProviderAccounts {
                provider: provider.to_string(),
                accounts: vec![summary],
            }),
        }
    }
    groups
}

/// The account a connect request describes, before anything is asked of the vault or the network.
pub(super) fn requested_account(
    request: ConnectProviderAccountRequest,
) -> Result<ProviderAccount, ProviderAccountError> {
    let preset = preset(&request.preset)
        .ok_or_else(|| ProviderAccountError::UnknownPreset(request.preset.clone()))?;
    if request.api_key.is_empty() {
        return Err(ProviderAccountError::MissingApiKey);
    }
    let endpoints = endpoints_for(
        preset,
        non_blank(&request.anthropic_base_url),
        non_blank(&request.openai_base_url),
    )?;
    Ok(ProviderAccount {
        account: account_id_for(preset, &request.label),
        preset,
        label: request.label,
        endpoints,
        api_key: SecretString::new(request.api_key),
    })
}

/// A request string with nothing in it is a field the client left unset.
fn non_blank(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

/// Every [`ProviderAccountError`] is a request the operator can correct.
pub(super) fn refusal_of_provider_account(error: ProviderAccountError) -> Status {
    let reason = match error {
        ProviderAccountError::UnknownPreset(preset) => {
            format!("{preset:?} is not a known provider")
        }
        ProviderAccountError::MissingBaseUrl { preset, protocol } => format!(
            "the {preset} provider needs a base URL for {}",
            protocol.as_str()
        ),
        ProviderAccountError::FixedBaseUrl { preset, protocol } => format!(
            "the {preset} provider's {} endpoint is fixed and cannot be replaced",
            protocol.as_str()
        ),
        ProviderAccountError::UnservedBaseUrl { preset, protocol } => format!(
            "the {preset} provider does not serve {}, so it takes no base URL for it",
            protocol.as_str()
        ),
        ProviderAccountError::InvalidBaseUrl(url) => format!(
            "{url:?} is not an http(s) URL with a host and without credentials, query or fragment"
        ),
        ProviderAccountError::MissingApiKey => "an API key is required".to_string(),
    };
    Status::invalid_argument(reason)
}

/// What a person may see of a record. The secret is reduced to whether one is present.
pub(super) fn summary_of(
    record: &CredentialRecord,
    sync_status: Option<&dyn SyncStatusSource>,
) -> AccountSummary {
    let sync_status = sync_status
        .and_then(|source| source.status_for(&record.provider, &record.account))
        .map_or(SyncStatus::Unspecified, sync_status_proto_of);
    let provider_account = ProviderAccount::from_record(record);
    AccountSummary {
        provider: record.provider.as_str().to_string(),
        account_id: record.account.as_str().to_string(),
        label: record.label.clone(),
        subject: record
            .metadata
            .get(META_SUBJECT)
            .cloned()
            .unwrap_or_default(),
        updated_at: i64::try_from(record.updated_at).unwrap_or(i64::MAX),
        has_secret: !record.secret.expose().is_empty(),
        sync_status: sync_status as i32,
        preset: provider_account
            .as_ref()
            .map_or_else(String::new, |account| account.preset.id.to_string()),
        default_for: defaults_of(record),
        wire_protocols: provider_account.map_or_else(Vec::new, |account| {
            account
                .wire_protocols()
                .into_iter()
                .map(|protocol| protocol.as_str().to_string())
                .collect()
        }),
    }
}

/// `#keyring` 6/9's aggregate summary, mapped 1:1 onto the wire enum. `SYNC_STATUS_UNSPECIFIED`
/// is reserved for "nothing has synced it yet" and is never produced here — that case is handled
/// by `summary_of`, before this function ever sees a value.
fn sync_status_proto_of(summary: AccountSyncSummary) -> SyncStatus {
    match summary {
        AccountSyncSummary::Synced => SyncStatus::Synced,
        AccountSyncSummary::Pending => SyncStatus::Pending,
        AccountSyncSummary::Undeliverable => SyncStatus::Undeliverable,
        AccountSyncSummary::Conflict => SyncStatus::Conflict,
        AccountSyncSummary::Refused => SyncStatus::Refused,
    }
}

/// The RPC status a refusal becomes wherever it is an error rather than an answer.
pub(super) fn status_for(refusal: AccountsError) -> Status {
    match refusal {
        AccountsError::NoSuchSession => {
            Status::unauthenticated("the session token names no signed-in session")
        }
        AccountsError::Locked => Status::failed_precondition(
            "your credential vault is locked on this daemon; \
             unlock it with your passphrase — nothing in it is lost",
        ),
        AccountsError::Uninitialized => Status::failed_precondition(
            "no credential vault exists for you on this daemon yet; \
             choosing a passphrase creates one",
        ),
        AccountsError::NotFound { provider, account } => Status::not_found(format!(
            "no {provider} account {account} is linked, so there is nothing to rename"
        )),
        AccountsError::Unavailable(reason) => Status::internal(reason),
    }
}

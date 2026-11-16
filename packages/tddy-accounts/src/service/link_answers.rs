use crate::linking::{record_for_link, LinkError, LinkedIdentity};
use crate::service::Linking;
use std::time::{SystemTime, UNIX_EPOCH};
use tddy_credentials::{CredentialRecord, ProviderId};
use tddy_rpc::Status;
use tddy_service::proto::accounts::{LinkState, PollLinkAccountResponse};

/// Deduplicate against what the vault holds and write the result. Mints nothing.
pub(super) fn store_link(
    linking: &Linking,
    session_token: &str,
    provider: &ProviderId,
    identity: &LinkedIdentity,
    access_token: &str,
) -> Result<CredentialRecord, LinkError> {
    let held = linking.store.held(session_token, provider)?;
    let linked_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| LinkError::Unavailable(error.to_string()))?
        .as_secs();
    let record = record_for_link(&held, provider, identity, access_token, linked_at);
    linking.store.put(session_token, record.clone())?;
    Ok(record)
}

pub(super) fn state_only(state: LinkState) -> PollLinkAccountResponse {
    PollLinkAccountResponse {
        state: state as i32,
        ..PollLinkAccountResponse::default()
    }
}

pub(super) fn pending(interval_seconds: u64) -> PollLinkAccountResponse {
    PollLinkAccountResponse {
        state: LinkState::LinkPending as i32,
        interval_seconds: saturating_i64(interval_seconds),
        ..PollLinkAccountResponse::default()
    }
}

pub(super) fn saturating_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

pub(super) fn link_status_for(refusal: LinkError) -> Status {
    match refusal {
        LinkError::NoSuchSession => {
            Status::unauthenticated("the session token names no signed-in session")
        }
        LinkError::Locked => Status::failed_precondition(
            "your credential vault is locked on this daemon; unlock it with your passphrase",
        ),
        LinkError::NoSuchLink => {
            Status::not_found("no link attempt by that id is in progress; begin a new one")
        }
        LinkError::UnsupportedProvider(provider) => {
            Status::invalid_argument(format!("accounts at {provider} cannot be linked"))
        }
        LinkError::Unavailable(reason) => Status::internal(reason),
    }
}

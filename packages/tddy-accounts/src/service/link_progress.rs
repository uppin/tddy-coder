use crate::linking::{LinkError, LinkProgress, LinkedIdentity};
use crate::service::account_answers::summary_of;
use crate::service::link_answers::link_status_for;
use crate::service::link_answers::pending;
use crate::service::link_answers::state_only;
use crate::service::link_answers::store_link;
use crate::service::AccountsServiceImpl;
use crate::service::Linking;
use tddy_credentials::{AccountId, ProviderId};
use tddy_rpc::Status;
use tddy_service::proto::accounts::{LinkState, PollLinkAccountRequest, PollLinkAccountResponse};

impl<S> AccountsServiceImpl<S> {
    /// The account the session was established with. `None` when no linking is wired, because
    /// nothing then knows which account that is.
    pub(super) fn session_account_of(
        &self,
        session_token: &str,
    ) -> Result<Option<(ProviderId, AccountId)>, Status> {
        self.linking.as_ref().map_or(Ok(None), |linking| {
            linking
                .store
                .session_account(session_token)
                .map_err(link_status_for)
        })
    }

    /// What the person is told about `progress`, finishing the attempt when it is terminal.
    pub(super) fn answer_for(
        &self,
        linking: &Linking,
        request: &PollLinkAccountRequest,
        provider: &ProviderId,
        progress: LinkProgress,
    ) -> Result<PollLinkAccountResponse, Status> {
        let finish = || {
            linking
                .attempts
                .finish(&request.link_id)
                .map_err(link_status_for)
        };
        match progress {
            LinkProgress::Pending { interval_seconds } => {
                linking
                    .attempts
                    .reschedule(&request.link_id, interval_seconds, (self.clock)())
                    .map_err(link_status_for)?;
                Ok(pending(interval_seconds))
            }
            LinkProgress::Denied => {
                finish()?;
                Ok(state_only(LinkState::LinkDenied))
            }
            LinkProgress::Expired => {
                finish()?;
                Ok(state_only(LinkState::LinkExpired))
            }
            LinkProgress::Approved {
                identity,
                access_token,
            } => {
                finish()?;
                self.linked(
                    linking,
                    &request.session_token,
                    provider,
                    &identity,
                    &access_token,
                )
            }
        }
    }

    /// Store an approved link and report it. A closed vault is an answer, not an error.
    fn linked(
        &self,
        linking: &Linking,
        session_token: &str,
        provider: &ProviderId,
        identity: &LinkedIdentity,
        access_token: &str,
    ) -> Result<PollLinkAccountResponse, Status> {
        match store_link(linking, session_token, provider, identity, access_token) {
            Ok(record) => Ok(PollLinkAccountResponse {
                state: LinkState::LinkLinked as i32,
                account: Some(summary_of(&record, self.sync_status.as_deref())),
                ..PollLinkAccountResponse::default()
            }),
            Err(LinkError::Locked) => Ok(state_only(LinkState::LinkVaultLocked)),
            Err(refusal) => Err(link_status_for(refusal)),
        }
    }
}

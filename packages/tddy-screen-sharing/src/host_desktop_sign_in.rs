//! Signing in to a host's desktop: what it advertises, what the operator answered, and what they
//! asked to be remembered (`#host-desktops` 8/8).
//!
//! - **Inspect.** `InspectHostDesktop` runs the target's bridge binary with `--inspect`
//!   (`tddy_screenshare::inspect`), writes `{host, port}` to its stdin and reads the one report line
//!   it prints: the sign-in methods the desktop advertises, read without signing in.
//! - **The answer.** A `DESKTOP_CREDENTIALS` prompt is answered with a protobuf-encoded
//!   `DesktopCredentialAnswer`, RSA-OAEP-encrypted under the host's published key exactly as a
//!   password answer is.
//! - **Remember.** An answer with `remember` set is a `screen-sharing` record in the operator's own
//!   credential vault, account `host:<daemon_instance_id>:<target_id>`. The next start uses it
//!   without asking; a desktop that refuses it marks it refused, and a refused one is not used again
//!   until the operator replaces it. An answer without `remember` is kept nowhere.
//!
//! A module of its own rather than more of `screen_sharing_service.rs`, which is due to be split
//! (`#host-desktops` node 1); the service's trait methods delegate here.

use std::sync::Arc;

use tddy_credentials::{AccountId, SessionVaults};
use tddy_host_service::host_keypair::HostKeypair;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::screen_sharing::{
    DesktopCredentialAnswer, InspectHostDesktopRequest, InspectHostDesktopResponse,
};

use crate::screen_sharing_records::TargetError;
use crate::screen_sharing_service::ScreenSharingServiceImpl;

/// `InspectHostDesktop`: the sign-in methods a host's desktop advertises, read by its bridge in
/// inspect mode, and whether the operator has a usable remembered credential for it.
pub async fn inspect_host_desktop(
    service: &ScreenSharingServiceImpl,
    request: Request<InspectHostDesktopRequest>,
) -> Result<Response<InspectHostDesktopResponse>, Status> {
    // TODO(sign-in-form): implement
    let _ = (service, request);
    todo!()
}

/// A `DESKTOP_CREDENTIALS` answer, from the ciphertext the operator's browser encrypted under this
/// host's published key.
///
/// Nothing here writes it anywhere or logs it; whether it is remembered is the caller's decision,
/// read off the answer's own `remember`.
pub fn decrypt_desktop_credential_answer(
    keypair: &dyn HostKeypair,
    encrypted_answer: &[u8],
) -> Result<DesktopCredentialAnswer, Status> {
    // TODO(sign-in-form): implement
    let _ = (keypair, encrypted_answer);
    todo!()
}

/// The account a remembered host-desktop credential occupies at the `screen-sharing` provider:
/// `host:<daemon_instance_id>:<target_id>`, so it never collides with a session target's account.
#[must_use]
pub fn remembered_account(daemon_instance_id: &str, target_id: &str) -> AccountId {
    // TODO(sign-in-form): implement
    let _ = (daemon_instance_id, target_id);
    todo!()
}

/// The host-desktop credentials operators asked to be remembered, in each operator's own vault.
pub struct RememberedDesktopCredentials {
    vaults: Arc<SessionVaults>,
}

impl RememberedDesktopCredentials {
    #[must_use]
    pub fn new(vaults: Arc<SessionVaults>) -> Self {
        Self { vaults }
    }

    /// The credential `operator` remembered for this desktop, unless there is none or the desktop
    /// refused it.
    pub fn usable(
        &self,
        operator: &str,
        daemon_instance_id: &str,
        target_id: &str,
    ) -> Result<Option<DesktopCredentialAnswer>, TargetError> {
        // TODO(sign-in-form): implement
        let _ = (&self.vaults, operator, daemon_instance_id, target_id);
        todo!()
    }

    /// Seal `answer` in `operator`'s vault as this desktop's remembered credential, replacing any
    /// earlier one — refused or not.
    pub fn remember(
        &self,
        operator: &str,
        daemon_instance_id: &str,
        target_id: &str,
        answer: &DesktopCredentialAnswer,
    ) -> Result<(), TargetError> {
        // TODO(sign-in-form): implement
        let _ = (
            &self.vaults,
            operator,
            daemon_instance_id,
            target_id,
            answer,
        );
        todo!()
    }

    /// The desktop refused the remembered credential: keep it, but never use it again until the
    /// operator replaces it.
    pub fn mark_refused(
        &self,
        operator: &str,
        daemon_instance_id: &str,
        target_id: &str,
    ) -> Result<(), TargetError> {
        // TODO(sign-in-form): implement
        let _ = (&self.vaults, operator, daemon_instance_id, target_id);
        todo!()
    }
}

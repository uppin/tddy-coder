//! Signing in to a host's desktop: what it advertises, what the operator answered, and what they
//! asked to be remembered (`#host-desktops` 8/8).
//!
//! - **Inspect.** `InspectHostDesktop` runs the target's bridge binary with `--inspect`
//!   (`tddy_daemon_extensions::desktop_inspect`), writes `{host, port}` to its stdin and reads the one report line
//!   it prints: the sign-in methods the desktop advertises, read without signing in.
//! - **The answer.** A `DESKTOP_CREDENTIALS` prompt is answered with a protobuf-encoded
//!   `DesktopCredentialAnswer`, RSA-OAEP-encrypted under the host's published key exactly as a
//!   password answer is.
//! - **Remember.** An answer with `remember` set is a `screen-sharing` record in the operator's own
//!   credential vault, account `host:<daemon_instance_id>:<target_id>`, written once the desktop
//!   has accepted it — a credential the desktop refuses is never kept. The next start uses it
//!   without asking; a desktop that refuses it marks it refused, and a refused one is not used again
//!   until the operator replaces it. An answer without `remember` is kept nowhere.
//!
//! A module of its own rather than more of `screen_sharing_service.rs`, which is due to be split
//! (`#host-desktops` node 1); the service's trait methods delegate here.

use std::collections::BTreeMap;
use std::process::{Output, Stdio};
use std::sync::Arc;
use std::time::Duration;

use prost::Message as _;
use tddy_credentials::{
    AccountId, CredentialRecord, SecretString, SessionVaults, VaultState, FIRST_VERSION,
};
use tddy_daemon_extensions::desktop_inspect::{
    read_inspect_report, AdvertisedMethod, InspectTarget, INSPECT_FLAG,
};
use tddy_daemon_kernel::config::{resolve_rdp_binary_path, resolve_vnc_binary_path, DaemonConfig};
use tddy_host_service::host_desktop_targets::HostDesktopTarget;
use tddy_host_service::host_keypair::HostKeypair;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::screen_sharing::{
    DesktopAuthMethod, DesktopCredentialAnswer, InspectHostDesktopRequest,
    InspectHostDesktopResponse, Protocol,
};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};

use crate::screen_sharing_records::{screen_sharing_provider, TargetError, META_USERNAME};
use crate::screen_sharing_service::{HostScope, ScreenSharingServiceImpl};
use crate::session_vault_target_store::{now_unix_seconds, open_vault, refusal_of};

/// How long an inspection may take: a little past the bridge's own limit on the desktop's answer,
/// so a desktop that stops answering is reported by the bridge, and a bridge that hangs is stopped
/// here.
const INSPECT_TIMEOUT: Duration = Duration::from_secs(35);

/// What every remembered host-desktop account starts with, so it is told apart from a session
/// target's account.
const REMEMBERED_ACCOUNT_PREFIX: &str = "host:";

const META_METHOD: &str = "method";
const META_DOMAIN: &str = "domain";
const META_REFUSED: &str = "refused";

/// `InspectHostDesktop`: the sign-in methods a host's desktop advertises, read by its bridge in
/// inspect mode, and whether the operator has a usable remembered credential for it.
///
/// An X11 display is served by an x11vnc this host starts for it, with no sign-in at all: it
/// advertises `NONE` without a bridge being run.
pub async fn inspect_host_desktop(
    service: &ScreenSharingServiceImpl,
    request: Request<InspectHostDesktopRequest>,
) -> Result<Response<InspectHostDesktopResponse>, Status> {
    let req = request.into_inner();
    let operator = service.require_user(&req.session_token)?;
    let scope = service.require_host_scope()?;
    let config = service.require_config()?;

    let target = find_host_target(scope, &req)?;

    let remembered = service
        .remembered_desktop_credentials()
        .map(|remembered| remembered.usable(&operator, &req.daemon_instance_id, &req.target_id))
        .transpose()
        .map_err(crate::screen_sharing_service::target_error_to_status)?
        .flatten()
        .is_some();

    if target.display.is_some() {
        return Ok(Response::new(response_for(
            &[AdvertisedMethod::None],
            remembered,
        )));
    }

    let binary = inspect_bridge_binary(config, &target)?;
    let methods = run_inspect_bridge(
        &binary,
        &InspectTarget {
            host: target.host,
            port: target.port,
        },
    )
    .await?;

    Ok(Response::new(response_for(&methods, remembered)))
}

/// The desktop target `req` names among the ones its host has.
fn find_host_target(
    scope: &HostScope,
    req: &InspectHostDesktopRequest,
) -> Result<HostDesktopTarget, Status> {
    scope
        .targets()
        .list(&req.daemon_instance_id)
        .map_err(Status::internal)?
        .into_iter()
        .find(|t| t.target_id == req.target_id)
        .ok_or_else(|| {
            Status::not_found(format!(
                "host {} has no desktop target {}",
                req.daemon_instance_id, req.target_id
            ))
        })
}

/// The bridge binary that speaks `target`'s protocol, which is the one that can inspect it.
fn inspect_bridge_binary(
    config: &DaemonConfig,
    target: &HostDesktopTarget,
) -> Result<String, Status> {
    match Protocol::try_from(target.protocol).unwrap_or(Protocol::Unspecified) {
        Protocol::Vnc => Ok(resolve_vnc_binary_path(config)),
        Protocol::Rdp => Ok(resolve_rdp_binary_path(config)),
        Protocol::Unspecified => Err(Status::failed_precondition(format!(
            "target {} names no protocol to inspect it with",
            target.target_id
        ))),
    }
}

/// Run `binary --inspect`, hand it where the desktop is, and read the one report line it prints.
///
/// Only an address crosses to the bridge: inspect never signs in, so there is no credential to
/// send.
async fn run_inspect_bridge(
    binary: &str,
    target: &InspectTarget,
) -> Result<Vec<AdvertisedMethod>, Status> {
    let child = spawn_inspect_bridge(binary, target).await?;

    let output = tokio::time::timeout(INSPECT_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| {
            Status::deadline_exceeded(format!(
                "the desktop at {}:{} did not report its sign-in methods within {INSPECT_TIMEOUT:?}",
                target.host, target.port
            ))
        })?
        .map_err(|e| Status::unavailable(format!("waiting for the bridge: {e}")))?;

    read_inspect_output(&output, target)
}

/// Spawn `binary --inspect` and write `target` to its stdin, closing it so the bridge starts.
async fn spawn_inspect_bridge(binary: &str, target: &InspectTarget) -> Result<Child, Status> {
    let request = serde_json::to_vec(target)
        .map_err(|e| Status::internal(format!("encoding the inspect request: {e}")))?;

    let mut child = Command::new(binary)
        .arg(INSPECT_FLAG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Dropped on a timeout, which must not leave the bridge running against the desktop.
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| Status::internal(format!("spawning the bridge binary '{binary}': {e}")))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| Status::internal("the bridge's stdin is not piped"))?;
    stdin
        .write_all(&request)
        .await
        .map_err(|e| Status::internal(format!("writing the inspect request to the bridge: {e}")))?;
    drop(stdin);
    Ok(child)
}

/// The methods in the one report line an inspecting bridge that exited cleanly printed.
fn read_inspect_output(
    output: &Output,
    target: &InspectTarget,
) -> Result<Vec<AdvertisedMethod>, Status> {
    if !output.status.success() {
        return Err(Status::unavailable(format!(
            "inspecting the desktop at {}:{} failed: {}",
            target.host,
            target.port,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .next()
        .ok_or_else(|| Status::unavailable("the bridge reported nothing about the desktop"))?;
    read_inspect_report(line)
        .map(|report| report.methods)
        .map_err(|e| Status::unavailable(format!("the bridge's report is unreadable: {e}")))
}

/// The response for `methods`: the ones an operator can sign in with, and the VNC security types
/// tddy cannot, by number.
fn response_for(methods: &[AdvertisedMethod], remembered: bool) -> InspectHostDesktopResponse {
    let mut offered = Vec::new();
    let mut unsupported_vnc_types = Vec::new();
    for method in methods {
        let offer = match method {
            AdvertisedMethod::None => DesktopAuthMethod::None,
            AdvertisedMethod::VncPassword => DesktopAuthMethod::VncPassword,
            AdvertisedMethod::VencryptPlain => DesktopAuthMethod::VencryptPlain,
            AdvertisedMethod::VencryptTlsPlain => DesktopAuthMethod::VencryptTlsPlain,
            AdvertisedMethod::AppleRemoteDesktop => DesktopAuthMethod::AppleRemoteDesktop,
            AdvertisedMethod::RdpTls => DesktopAuthMethod::RdpTls,
            AdvertisedMethod::RdpNla => DesktopAuthMethod::RdpNla,
            AdvertisedMethod::UnsupportedVnc { security_type } => {
                unsupported_vnc_types.push(u32::from(*security_type));
                continue;
            }
        };
        offered.push(offer as i32);
    }
    InspectHostDesktopResponse {
        offered,
        unsupported_vnc_types,
        remembered,
    }
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
    let plaintext = keypair
        .decrypt(encrypted_answer)
        .map_err(Status::permission_denied)?;
    // The decode error is left out: it can quote the bytes it choked on, and these are a password.
    DesktopCredentialAnswer::decode(plaintext.as_slice())
        .map_err(|_| Status::invalid_argument("that desktop credential is not a sign-in answer"))
}

/// The account a remembered host-desktop credential occupies at the `screen-sharing` provider:
/// `host:<daemon_instance_id>:<target_id>`, so it never collides with a session target's account.
#[must_use]
pub fn remembered_account(daemon_instance_id: &str, target_id: &str) -> AccountId {
    AccountId::new(format!(
        "{REMEMBERED_ACCOUNT_PREFIX}{daemon_instance_id}:{target_id}"
    ))
}

/// Whether `account` holds a remembered host-desktop credential rather than a session target.
#[must_use]
pub fn is_remembered_desktop_account(account: &AccountId) -> bool {
    account.as_str().starts_with(REMEMBERED_ACCOUNT_PREFIX)
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
    ///
    /// An operator with no vault yet has remembered nothing, and one whose vault is sealed has
    /// nothing usable remembered right now: either way they are asked, so opening a host desktop
    /// never depends on the vault being open.
    pub fn usable(
        &self,
        operator: &str,
        daemon_instance_id: &str,
        target_id: &str,
    ) -> Result<Option<DesktopCredentialAnswer>, TargetError> {
        if self.vaults.state(operator) != VaultState::Open {
            return Ok(None);
        }
        let Some(record) = self.record(operator, daemon_instance_id, target_id)? else {
            return Ok(None);
        };
        if record.metadata.contains_key(META_REFUSED) {
            return Ok(None);
        }
        answer_from(&record).map(Some)
    }

    /// Whether `operator`'s vault is open to take a remembered credential, without writing one.
    ///
    /// Asked before a bridge starts, so an operator who ticked Remember with a sealed vault is
    /// told it cannot be done rather than finding out at the next open.
    pub fn can_remember(&self, operator: &str) -> Result<(), TargetError> {
        open_vault(&self.vaults, operator).map(drop)
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
        let vault = open_vault(&self.vaults, operator)?;
        let account = remembered_account(daemon_instance_id, target_id);
        let version = next_version(
            vault
                .get(&screen_sharing_provider(), &account)
                .map_err(|error| refusal_of(operator, error))?
                .as_ref(),
        );
        let metadata = BTreeMap::from([
            (META_METHOD.to_string(), method_name(answer.method)?),
            (META_USERNAME.to_string(), answer.username.clone()),
            (META_DOMAIN.to_string(), answer.domain.clone()),
        ]);
        vault
            .put(CredentialRecord {
                provider: screen_sharing_provider(),
                account,
                label: format!("{target_id} on {daemon_instance_id}"),
                secret: SecretString::new(&answer.password),
                metadata,
                updated_at: now_unix_seconds()?,
                version,
            })
            .map_err(|error| refusal_of(operator, error))
    }

    /// The desktop refused the remembered credential: keep it, but never use it again until the
    /// operator replaces it.
    pub fn mark_refused(
        &self,
        operator: &str,
        daemon_instance_id: &str,
        target_id: &str,
    ) -> Result<(), TargetError> {
        let Some(mut record) = self.record(operator, daemon_instance_id, target_id)? else {
            return Ok(());
        };
        record
            .metadata
            .insert(META_REFUSED.to_string(), "true".to_string());
        record.version = next_version(Some(&record));
        record.updated_at = now_unix_seconds()?;
        open_vault(&self.vaults, operator)?
            .put(record)
            .map_err(|error| refusal_of(operator, error))
    }

    fn record(
        &self,
        operator: &str,
        daemon_instance_id: &str,
        target_id: &str,
    ) -> Result<Option<CredentialRecord>, TargetError> {
        open_vault(&self.vaults, operator)?
            .get(
                &screen_sharing_provider(),
                &remembered_account(daemon_instance_id, target_id),
            )
            .map_err(|error| refusal_of(operator, error))
    }
}

/// The version a record replacing `existing` carries.
fn next_version(existing: Option<&CredentialRecord>) -> u64 {
    existing.map_or(FIRST_VERSION, |record| record.version + 1)
}

fn method_name(method: i32) -> Result<String, TargetError> {
    DesktopAuthMethod::try_from(method)
        .map(|method| method.as_str_name().to_string())
        .map_err(|_| TargetError::Malformed(format!("{method} is not a sign-in method")))
}

fn answer_from(record: &CredentialRecord) -> Result<DesktopCredentialAnswer, TargetError> {
    let field = |key: &str| record.metadata.get(key).cloned().unwrap_or_default();
    let method = record
        .metadata
        .get(META_METHOD)
        .ok_or_else(|| TargetError::Malformed(format!("missing {META_METHOD}")))?;
    let method = DesktopAuthMethod::from_str_name(method)
        .ok_or_else(|| TargetError::Malformed(format!("{method} is not a sign-in method")))?;
    Ok(DesktopCredentialAnswer {
        method: method as i32,
        username: field(META_USERNAME),
        domain: field(META_DOMAIN),
        password: record.secret.expose().to_string(),
        remember: true,
    })
}

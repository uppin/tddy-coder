//! A session's provider route, resolved once at its start or resume.
//!
//! The account is read from the session owner's vault exactly as the GitHub identity is
//! ([`SessionAccountAccess`]), and with the same refusals: a locked vault, no vault, or an account
//! it does not hold stop the start before anything is spawned. **No fallback**: a session asked to
//! run on an account never runs on the harness's vendor instead.
//!
//! Unlike the GitHub token, a provider key does reach the agent's environment — a harness CLI reads
//! its key from there and has no per-call socket to ask instead. It goes to the agent child only:
//! [`pty_route_env`] for a PTY, the jail's secret files for a sandbox, the route file for a tool
//! session.

use std::path::Path;

use tddy_accounts::AccountsError;
use tddy_core::provider_route::{
    choose_route, resolve_route, Harness, HarnessRoute, RouteChoice, RouteEndpoints, RouteRefusal,
    RouteSecret, RouteSources, RouteTarget,
};
use tddy_core::{
    read_session_provider_account, write_session_provider_account, SessionProviderAccount,
};
use tddy_rpc::Status;
use tddy_service::proto::session::StartSessionRequest;

use crate::session_acting_identity::{SessionAccountAccess, SessionIdentityRefusal};

/// The session-level choice a start request carries. The other levels are filled by their owners.
#[must_use]
pub fn session_route_sources(req: &StartSessionRequest) -> RouteSources {
    // The proto makes the two exclusive; if a caller sends both, the named account is the more
    // specific choice and wins — never the vendor the operator did not ask for.
    let session = if !req.provider_account_id.is_empty() {
        Some(RouteChoice::Account(req.provider_account_id.clone()))
    } else if req.provider_built_in {
        Some(RouteChoice::BuiltIn)
    } else {
        None
    };
    RouteSources {
        session,
        ..RouteSources::default()
    }
}

/// [`session_route_sources`] plus the owner's default for `harness` — what a start that may run on
/// a provider account is decided from. A locked vault refuses rather than starting on the vendor
/// the person's default may have been chosen to avoid.
///
/// The default is read only when it can matter: a start that already names an account or
/// "built in" never consults the vault for it, so an explicit choice is never refused for a vault
/// it does not need.
pub fn session_route_sources_with_default(
    req: &StartSessionRequest,
    harness: &str,
    access: &SessionAccountAccess,
) -> Result<RouteSources, Status> {
    let sources = session_route_sources(req);
    if sources.session.is_some() || sources.agent_definition.is_some() {
        return Ok(sources);
    }
    let user_default = access
        .default_provider_account(harness)
        .map_err(status_of_refusal)?;
    Ok(RouteSources {
        user_default,
        ..sources
    })
}

/// The route `harness` runs `model` on, after precedence — `None` for the harness's vendor.
///
/// Reads the chosen account from the session owner's vault; refuses when it cannot be read, is not
/// a provider account, no model was chosen, or the harness cannot run on it.
pub fn resolve_session_route(
    access: &SessionAccountAccess,
    harness: Harness,
    sources: &RouteSources,
    model: &str,
) -> Result<Option<HarnessRoute>, Status> {
    choose_route(sources)
        .map(|account_id| route_on_account(access, harness, &account_id, model))
        .transpose()
}

/// The route `harness` runs `model` on the account `account_id` names in the owner's vault.
pub(crate) fn route_on_account(
    access: &SessionAccountAccess,
    harness: Harness,
    account_id: &str,
    model: &str,
) -> Result<HarnessRoute, Status> {
    // Every model alias the harness could fall through to is pinned to this one; an empty pin
    // would leave the harness choosing a vendor model name the account does not serve.
    if model.trim().is_empty() {
        return Err(Status::invalid_argument(format!(
            "a session on provider account {account_id:?} needs a model: choose one the account serves"
        )));
    }
    let account = access
        .provider_account(account_id)
        .map_err(status_of_refusal)?;
    let target = RouteTarget {
        preset: account.preset.id.to_string(),
        endpoints: RouteEndpoints {
            anthropic_messages: account.endpoints.anthropic_messages,
            openai_responses: account.endpoints.openai_responses,
            openai_chat: account.endpoints.openai_chat,
        },
        api_key: RouteSecret::new(account.api_key.expose()),
        model: model.to_string(),
    };
    resolve_route(harness, &target).map_err(|refusal| match refusal {
        RouteRefusal::NotYetRoutable(_) => Status::unimplemented(refusal.to_string()),
        other => Status::invalid_argument(other.to_string()),
    })
}

/// A vault refusal as the status a start answers with: the same codes whichever account kind was
/// asked for.
pub(crate) fn status_of_refusal(refusal: SessionIdentityRefusal) -> Status {
    let message = refusal.to_string();
    match refusal {
        SessionIdentityRefusal::NoVaultsConfigured => Status::failed_precondition(message),
        SessionIdentityRefusal::Vault(AccountsError::NoSuchSession) => {
            Status::unauthenticated(message)
        }
        SessionIdentityRefusal::Vault(AccountsError::Locked | AccountsError::Uninitialized) => {
            Status::failed_precondition(message)
        }
        SessionIdentityRefusal::Vault(AccountsError::NotFound { .. }) => Status::not_found(message),
        SessionIdentityRefusal::Vault(AccountsError::Unavailable(_)) => Status::internal(message),
        SessionIdentityRefusal::Identity(_) => Status::failed_precondition(message),
        SessionIdentityRefusal::AmbiguousLabel(_) => Status::invalid_argument(message),
    }
}

/// The environment a PTY-hosted agent is launched with for `route`: its plain and secret pairs.
/// Files and arguments are not environment and are applied by the caller.
#[must_use]
pub fn pty_route_env(route: &HarnessRoute) -> Vec<(String, String)> {
    route
        .env
        .iter()
        .cloned()
        .chain(
            route
                .secret_env
                .iter()
                .map(|(name, secret)| (name.clone(), secret.expose().to_string())),
        )
        .collect()
}

/// What a resumed session runs on: the account and model its start recorded beside `.session.yaml`
/// (`provider-account.yaml`), as the session-level choice; the vendor when nothing was recorded.
/// The key is not in the record — [`resolve_session_route`] reads it from the vault again.
pub fn route_sources_on_resume(
    session_dir: &Path,
) -> Result<(RouteSources, Option<String>), Status> {
    let recorded = read_session_provider_account(session_dir)
        .map_err(|e| Status::internal(format!("provider-account record unreadable: {e}")))?;
    Ok(match recorded {
        Some(SessionProviderAccount { account_id, model }) => (
            RouteSources {
                session: Some(RouteChoice::Account(account_id)),
                ..RouteSources::default()
            },
            Some(model),
        ),
        None => (RouteSources::default(), None),
    })
}

/// What a start that chose an account is launched with, and what it records for the resume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartRoute {
    /// The pairs the PTY is launched with — see [`pty_route_env`].
    pub env: Vec<(String, String)>,
    /// The account and model the session's directory records; never the key.
    pub account: SessionProviderAccount,
}

impl StartRoute {
    /// Record the account beside the session's `.session.yaml`, so a resume re-applies it.
    pub fn record_in(&self, session_dir: &Path) -> Result<(), Status> {
        write_session_provider_account(session_dir, &self.account)
            .map_err(|e| Status::internal(format!("failed to record the provider account: {e}")))
    }
}

/// The route a start of `harness` on `model` is launched with, resolved before anything is created;
/// `None` when no account was chosen, so the harness runs on its vendor.
pub fn start_route(
    access: &SessionAccountAccess,
    harness: Harness,
    sources: &RouteSources,
    model: &str,
) -> Result<Option<StartRoute>, Status> {
    let Some(account_id) = choose_route(sources) else {
        return Ok(None);
    };
    let route = route_on_account(access, harness, &account_id, model)?;
    Ok(Some(StartRoute {
        env: pty_route_env(&route),
        account: SessionProviderAccount {
            account_id,
            model: model.to_string(),
        },
    }))
}

/// The route a resumed session runs on: the account its start recorded, re-resolved against the
/// owner's vault as it stands now. `None` for a session on its vendor.
pub fn resume_route(
    access: &SessionAccountAccess,
    harness: Harness,
    session_dir: &Path,
) -> Result<Option<HarnessRoute>, Status> {
    let (sources, model) = route_sources_on_resume(session_dir)?;
    let Some(model) = model else {
        return Ok(None);
    };
    resolve_session_route(access, harness, &sources, &model)
}

/// The environment a resumed PTY session is relaunched with: the route its start recorded,
/// re-resolved against the owner's vault as it stands now. Empty for a session on its vendor.
pub fn resume_route_env(
    access: &SessionAccountAccess,
    harness: Harness,
    session_dir: &Path,
) -> Result<Vec<(String, String)>, Status> {
    Ok(resume_route(access, harness, session_dir)?
        .map(|route| pty_route_env(&route))
        .unwrap_or_default())
}

/// The route a sandboxed start of `harness` on `model` runs on; `None` when no account was chosen.
///
/// The route is resolved before `session_dir` is created, so a refusal leaves nothing on disk. The
/// account and model it runs on are then recorded in `session_dir` (never the key), so a resume
/// re-applies them. The jail receives the route through its secret files and environment.
pub fn sandboxed_start_route(
    access: &SessionAccountAccess,
    harness: Harness,
    sources: &RouteSources,
    model: &str,
    session_dir: &Path,
) -> Result<Option<HarnessRoute>, Status> {
    let Some(account_id) = choose_route(sources) else {
        return Ok(None);
    };
    let route = route_on_account(access, harness, &account_id, model)?;
    std::fs::create_dir_all(session_dir)
        .map_err(|e| Status::internal(format!("failed to create session dir: {e}")))?;
    write_session_provider_account(
        session_dir,
        &SessionProviderAccount {
            account_id,
            model: model.to_string(),
        },
    )
    .map_err(|e| Status::internal(format!("failed to record the provider account: {e}")))?;
    Ok(Some(route))
}

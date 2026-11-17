//! A derived host, named by its id, resolved for a caller into what a start is addressed with
//! (`#remote-shell` 11/11).
//!
//! The orchestrator's `start_session` tool names a derived host by the `id` the start options
//! listed. The daemon turns that id back into the host's parent and its `SessionTarget` by listing
//! the derived hosts **for the caller** — the same sources, the same vault — so an agent can only
//! ever start on a host the caller could have chosen in the New-session form, and never composes
//! a target of its own.
//!
//! Resolution lists this daemon's derived hosts for the caller and picks the one the id names.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-16-remote-shell-orchestrator-target-start.md

use std::fmt;

use tddy_rpc::{Code, Request};
use tddy_service::proto::host::{HostService, ListDerivedHostsRequest, ProbeOutcome};
use tddy_service::proto::session::SessionTarget;

use super::HostServiceImpl;

/// A derived host as a start is addressed at it: the daemon the session starts on, and the target
/// its agent runs on.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedDerivedHost {
    /// The parent daemon: `StartSessionRequest.daemon_instance_id`.
    pub parent_instance_id: String,
    /// The host: `StartSessionRequest.target`.
    pub target: SessionTarget,
}

/// Why a derived-host id was not resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DerivedHostResolutionError {
    /// The caller's session token names no signed-in session.
    Unauthenticated,
    /// The hosts could not be listed. Carries the listing's reason.
    Listing(String),
    /// No derived host of the caller's has this id. Carries the ids that were listed, so the agent
    /// is told what it can name.
    Unknown { id: String, listed: Vec<String> },
    /// The host is listed but cannot be used now. Carries the daemon's reason.
    Unusable { id: String, reason: String },
}

impl fmt::Display for DerivedHostResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unauthenticated => f.write_str("invalid or expired session"),
            Self::Listing(reason) => write!(f, "the derived hosts could not be listed: {reason}"),
            Self::Unknown { id, listed } if listed.is_empty() => {
                write!(
                    f,
                    "no derived host is called {id:?}: this host reaches none"
                )
            }
            Self::Unknown { id, listed } => write!(
                f,
                "no derived host is called {id:?}; the hosts reached are: {}",
                listed.join(", ")
            ),
            Self::Unusable { id, reason } => {
                write!(f, "the derived host {id:?} cannot be used now: {reason}")
            }
        }
    }
}

impl std::error::Error for DerivedHostResolutionError {}

impl HostServiceImpl {
    /// The derived host `id` names, for the caller `session_token` signs in: its parent and its
    /// target, or why it is not one the caller can start on.
    ///
    /// Lists this daemon's derived hosts for the caller — SSH aliases from the operator's config,
    /// Coder workspaces from the caller's own vault — and picks the one whose `id` matches.
    pub async fn resolve_derived_host(
        &self,
        session_token: &str,
        id: &str,
    ) -> Result<ResolvedDerivedHost, DerivedHostResolutionError> {
        let answer = HostService::list_derived_hosts(
            self,
            Request::direct(ListDerivedHostsRequest {
                session_token: session_token.to_string(),
                daemon_instance_id: String::new(),
            }),
        )
        .await
        .map_err(|status| {
            if status.code == Code::Unauthenticated {
                DerivedHostResolutionError::Unauthenticated
            } else {
                DerivedHostResolutionError::Listing(status.message().to_string())
            }
        })?
        .into_inner();
        if answer.outcome == ProbeOutcome::Failed as i32 {
            let reason = if answer.failure_reason.is_empty() {
                "the derived hosts could not be listed".to_string()
            } else {
                answer.failure_reason
            };
            return Err(DerivedHostResolutionError::Listing(reason));
        }
        let listed: Vec<String> = answer.hosts.iter().map(|host| host.id.clone()).collect();
        let Some(host) = answer.hosts.into_iter().find(|host| host.id == id) else {
            return Err(DerivedHostResolutionError::Unknown {
                id: id.to_string(),
                listed,
            });
        };
        if !host.usable {
            return Err(DerivedHostResolutionError::Unusable {
                id: host.id,
                reason: host.unusable_reason,
            });
        }
        match host.target {
            Some(target) => Ok(ResolvedDerivedHost {
                parent_instance_id: host.parent_instance_id,
                target,
            }),
            None => Err(DerivedHostResolutionError::Unusable {
                id: host.id,
                reason: "the host lists no target a session can start on".to_string(),
            }),
        }
    }
}

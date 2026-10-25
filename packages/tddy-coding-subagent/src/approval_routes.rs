//! Which headless conversation a permission approval is for, and which caller may answer it.
//!
//! A headless coding agent asks permission through the daemon, which parks the request on the
//! conversation's [`ParkedPermissions`] until the request is answered or cancelled. The caller that
//! asks is the agent's own tool server, presenting the credential minted for that agent. The minted
//! credential carries no session or conversation claim, so the binding is made when the conversation
//! launches: the launch registers the session it runs for and the credential its agent was minted,
//! and an approval is accepted only when it presents both.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § Coding agents

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tddy_agent_backend::{PermissionDecision, PermissionOptionView, PermissionRequest};

use crate::parked_permissions::ParkedPermissions;

/// The option an approval allows the tool call with.
const ALLOW: &str = "allow";
/// The option an approval rejects the tool call with.
const REJECT: &str = "reject";
/// How long a request waits for its conversation to answer it, unless the routes are built with
/// another bound.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// The approval routes of one daemon: every headless conversation it has launched and not closed.
///
/// Shared as an `Arc` between the launcher, the open and close paths of the conversations, and the
/// exec-tool handler that answers approvals.
pub struct ApprovalRoutes {
    routes: Mutex<HashMap<String, Route>>,
    /// How long a request waits for its conversation to answer it before it is denied.
    timeout: Duration,
}

impl Default for ApprovalRoutes {
    fn default() -> Self {
        Self {
            routes: Mutex::new(HashMap::new()),
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

#[derive(Default)]
struct Route {
    /// Set by [`ApprovalRoutes::register_launch`]: the session the conversation runs for, and the
    /// credential its agent was minted with. Never logged.
    launch: Option<Launch>,
    /// Set by [`ApprovalRoutes::register_permissions`] once the session is open.
    permissions: Option<Arc<ParkedPermissions>>,
}

struct Launch {
    session_id: String,
    session_token: String,
}

impl ApprovalRoutes {
    fn routes(&self) -> MutexGuard<'_, HashMap<String, Route>> {
        self.routes.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Deny a request that its conversation does not answer within `timeout` (builder). Defaults to
    /// 60 seconds.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Bind `conversation_id` to `session_id`, and to the credential `session_token` its agent was
    /// minted with, so only a caller presenting both may answer for it.
    pub fn register_launch(&self, conversation_id: &str, session_id: &str, session_token: &str) {
        self.routes()
            .entry(conversation_id.to_string())
            .or_default()
            .launch = Some(Launch {
            session_id: session_id.to_string(),
            session_token: session_token.to_string(),
        });
    }

    /// Route approvals for `conversation_id` to `permissions`, the handle its open session parks
    /// its agent's permission requests on.
    pub fn register_permissions(&self, conversation_id: &str, permissions: Arc<ParkedPermissions>) {
        self.routes()
            .entry(conversation_id.to_string())
            .or_default()
            .permissions = Some(permissions);
    }

    /// Forget `conversation_id`, so a later approval for it is refused. A request still parked on it
    /// is cancelled, so the agent is not left waiting on an answer nothing will give.
    pub fn remove(&self, conversation_id: &str) {
        let removed = self.routes().remove(conversation_id);
        if let Some(Route {
            permissions: Some(permissions),
            ..
        }) = removed
        {
            permissions.cancel_pending();
        }
    }

    /// Ask permission for `tool_name` with `input` on behalf of the caller presenting `session_id`
    /// and `session_token`, and wait up to the routes' timeout for the conversation to answer.
    ///
    /// `Err` means the approval was refused before anything parked: the conversation is unknown, has
    /// no open session, or does not belong to the caller. `Ok(true)` is an allow; `Ok(false)` is a
    /// rejection, a cancellation or a timeout, and a request that timed out is cancelled so it is not
    /// left parked.
    pub async fn request(
        &self,
        conversation_id: &str,
        session_id: &str,
        session_token: &str,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> Result<bool, String> {
        let permissions = self.owner_of(conversation_id, session_id, session_token)?;
        let request = PermissionRequest {
            session_id: session_id.to_string(),
            tool_call_id: tool_name.to_string(),
            title: Some(tool_name.to_string()),
            raw_input: Some(input.clone()),
            options: vec![
                PermissionOptionView {
                    option_id: ALLOW.to_string(),
                    name: "Allow".to_string(),
                    kind: "allow_once".to_string(),
                },
                PermissionOptionView {
                    option_id: REJECT.to_string(),
                    name: "Reject".to_string(),
                    kind: "reject_once".to_string(),
                },
            ],
        };
        let (request_id, answered) = permissions.park(request);
        // Until the request is answered, dropping this approval cancels that request alone. A
        // client that disconnects drops the call in flight, and the agent must not be left waiting
        // on an answer nobody is waiting to give.
        let mut parking = Parking {
            permissions: Arc::clone(&permissions),
            request_id,
            armed: true,
        };
        let timeout = self.timeout;
        match tokio::time::timeout(timeout, answered).await {
            Ok(Ok(PermissionDecision::Selected { option_id })) => {
                parking.disarm();
                Ok(option_id == ALLOW)
            }
            Ok(Ok(PermissionDecision::Cancelled)) | Ok(Err(_)) => {
                parking.disarm();
                Ok(false)
            }
            Err(_elapsed) => {
                // Still armed: returning drops `parking`, which cancels this request alone.
                log::info!(
                    "a permission for tool {tool_name} on conversation {conversation_id} was not \
                     answered within {timeout:?}; it is denied"
                );
                Ok(false)
            }
        }
    }

    /// The permissions `conversation_id` answers from, when the caller owns it.
    fn owner_of(
        &self,
        conversation_id: &str,
        session_id: &str,
        session_token: &str,
    ) -> Result<Arc<ParkedPermissions>, String> {
        let routes = self.routes();
        let route = routes.get(conversation_id).ok_or_else(|| {
            format!("conversation '{conversation_id}' is not open on this daemon")
        })?;
        let (Some(launch), Some(permissions)) = (&route.launch, &route.permissions) else {
            return Err(format!(
                "conversation '{conversation_id}' has no open session to answer a permission for"
            ));
        };
        if !same_credential(&launch.session_id, session_id)
            || !same_credential(&launch.session_token, session_token)
        {
            return Err(format!(
                "conversation '{conversation_id}' does not belong to the session presenting this \
                 credential"
            ));
        }
        Ok(Arc::clone(permissions))
    }
}

/// A request parked for an approval that is still waiting on it. Dropped while still armed, it
/// cancels exactly that request; disarmed once the request has been answered, when there is nothing
/// left to cancel.
struct Parking {
    permissions: Arc<ParkedPermissions>,
    request_id: String,
    armed: bool,
}

impl Parking {
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for Parking {
    fn drop(&mut self) {
        if self.armed {
            self.permissions.cancel_request(&self.request_id);
        }
    }
}

/// Whether `a` and `b` are the same. Every byte is compared, so the time taken does not reveal how
/// much of a credential matched; only a difference in length returns early.
fn same_credential(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

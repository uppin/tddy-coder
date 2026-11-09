//! The permission requests an ACP agent asks the driver, and the handler that answers them.

/// One option the agent offers on a permission request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionOptionView {
    pub option_id: String,
    pub name: String,
    /// `allow_once`, `allow_always`, `reject_once` or `reject_always`.
    pub kind: String,
}

/// A `session/request_permission` from the agent.
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionRequest {
    pub session_id: String,
    pub tool_call_id: String,
    pub title: Option<String>,
    pub raw_input: Option<serde_json::Value>,
    /// ACP's tool kind of the call asking, by its wire name (`edit`, `execute`, `read`, …) — the
    /// spelling [`PermissionOptionView::kind`] uses for an option's kind. `None` when the agent named
    /// none.
    pub kind: Option<String>,
    pub options: Vec<PermissionOptionView>,
}

/// The answer to a [`PermissionRequest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    Selected { option_id: String },
    Cancelled,
}

/// Answers the agent's permission requests. The driver asks it and sends its decision verbatim.
#[async_trait::async_trait]
pub trait PermissionHandler: Send + Sync {
    async fn decide(&self, request: PermissionRequest) -> PermissionDecision;
}

/// Picks the first `allow_once` option — what the workflow backends have always answered.
#[derive(Debug, Clone, Copy, Default)]
pub struct AllowOnce;

/// The kind ACP gives an option that allows one call.
pub(super) const ALLOW_ONCE_KIND: &str = "allow_once";

#[async_trait::async_trait]
impl PermissionHandler for AllowOnce {
    async fn decide(&self, request: PermissionRequest) -> PermissionDecision {
        match request
            .options
            .into_iter()
            .find(|option| option.kind == ALLOW_ONCE_KIND)
        {
            Some(option) => PermissionDecision::Selected {
                option_id: option.option_id,
            },
            None => PermissionDecision::Cancelled,
        }
    }
}

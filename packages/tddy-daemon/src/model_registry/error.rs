//! The one error type the registry's store, provider clients and pure mappers all speak.

use tddy_rpc::Status;

/// Everything that can go wrong in the model registry. Deliberately typed rather than
/// `anyhow::Error`: the service maps each variant to a distinct RPC code, and a caller that must
/// tell "the provider is down" from "you asked for a tool that does not exist" cannot do that from
/// a message string.
#[derive(Debug)]
pub enum ModelRegistryError {
    /// The registry's own SQLite storage failed.
    Storage(sqlx::Error),
    /// A row with the same identity already exists (duplicate base URL, duplicate assistant name).
    AlreadyExists(String),
    /// The named row is not in this daemon's registry.
    NotFound(String),
    /// The row is still referenced by another row; removing it would silently drop that reference.
    InUse(String),
    /// A tool name outside `tddy_tool_engine::tool_catalog()`.
    UnknownTool(String),
    /// A field whose value could never work — an assistant name `--agent` can never match.
    InvalidName(String),
    /// A base URL this daemon must not be pointed at (a scheme it does not speak, or credentials
    /// embedded in the URL).
    InvalidBaseUrl(String),
    /// The directory a chat asked to run its tools in is not a usable one: empty, relative, or not
    /// a directory on this host. Distinct from [`Self::PermissionDenied`], which is the answer for
    /// a real directory the caller may not reach.
    InvalidWorkspace(String),
    /// The row belongs to another operator: everyone reads the registry, only the owner writes.
    PermissionDenied(String),
    /// The provider endpoint itself failed (unreachable, non-2xx, unparseable payload).
    Provider(String),
    /// The operation has no meaning for this provider kind (residency on a cloud provider).
    UnsupportedOperation(String),
}

/// How much of a provider's own words is kept in a message this daemon stores and returns.
///
/// A failing endpoint answers with whatever it likes — an HTML error page is hundreds of
/// kilobytes. That text is persisted on the provider row and returned by *every* `ListProviders`,
/// and a payload past ~60 KB is chunk-framed over LiveKit, where one lost frame wedges the call
/// with no error at all. A few hundred bytes is enough to diagnose a 502 and small enough that no
/// response is ever built out of it.
pub const MAX_PROVIDER_DETAIL_BYTES: usize = 400;

/// `text`, cut to [`MAX_PROVIDER_DETAIL_BYTES`] on a character boundary, saying so when it cut.
pub fn truncate_provider_detail(text: &str) -> String {
    if text.len() <= MAX_PROVIDER_DETAIL_BYTES {
        return text.to_string();
    }
    let mut end = MAX_PROVIDER_DETAIL_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}… (truncated, {} bytes in total)",
        &text[..end],
        text.len()
    )
}

impl std::fmt::Display for ModelRegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Deliberately says nothing about the failure itself: this string reaches operators
            // (an RPC status, an ACP error frame), and sqlx's own message carries the database
            // path for an `Io` fault and the constraint and column names for a `Database` one.
            // The detail is logged instead, where it is useful and stays on the host.
            ModelRegistryError::Storage(_) => write!(f, "{STORAGE_FAILED}"),
            ModelRegistryError::AlreadyExists(m) => write!(f, "already exists: {m}"),
            ModelRegistryError::NotFound(m) => write!(f, "not found: {m}"),
            ModelRegistryError::InUse(m) => write!(f, "still in use: {m}"),
            ModelRegistryError::UnknownTool(m) => write!(f, "unknown tool: {m}"),
            ModelRegistryError::InvalidName(m) => write!(f, "invalid name: {m}"),
            ModelRegistryError::InvalidBaseUrl(m) => write!(f, "invalid base url: {m}"),
            ModelRegistryError::InvalidWorkspace(m) => write!(f, "invalid workspace: {m}"),
            ModelRegistryError::PermissionDenied(m) => write!(f, "permission denied: {m}"),
            ModelRegistryError::Provider(m) => write!(f, "{m}"),
            ModelRegistryError::UnsupportedOperation(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for ModelRegistryError {}

impl From<sqlx::Error> for ModelRegistryError {
    fn from(e: sqlx::Error) -> Self {
        ModelRegistryError::Storage(e)
    }
}

/// What a caller is told when this daemon's own storage failed. The cause is in the daemon log.
pub const STORAGE_FAILED: &str = "the model registry's storage failed; see the daemon log";

impl From<ModelRegistryError> for Status {
    fn from(e: ModelRegistryError) -> Self {
        match e {
            ModelRegistryError::Storage(inner) => {
                // The only place the sqlx detail is allowed to go: the host's log, never a
                // response. It names the database path and the constraint that failed.
                log::error!(
                    target: "tddy_daemon::model_registry",
                    "model registry storage failed: {inner}"
                );
                Status::internal(STORAGE_FAILED)
            }
            ModelRegistryError::AlreadyExists(m) => Status::already_exists(m),
            ModelRegistryError::NotFound(m) => Status::not_found(m),
            // A refusal the caller can act on by removing the referencing row first.
            ModelRegistryError::InUse(m) => Status::failed_precondition(m),
            ModelRegistryError::UnknownTool(m) => {
                Status::invalid_argument(format!("unknown tool: {m}"))
            }
            ModelRegistryError::InvalidName(m) => Status::invalid_argument(m),
            ModelRegistryError::InvalidBaseUrl(m) => Status::invalid_argument(m),
            ModelRegistryError::InvalidWorkspace(m) => Status::invalid_argument(m),
            // Reads are fleet-wide; writes, and the credential, belong to the row's owner.
            ModelRegistryError::PermissionDenied(m) => Status::permission_denied(m),
            // The provider endpoint, not this daemon, is what failed — say so verbatim so the
            // screen can render the cause instead of a generic "internal error".
            ModelRegistryError::Provider(m) => Status::unavailable(m),
            ModelRegistryError::UnsupportedOperation(m) => Status::failed_precondition(m),
        }
    }
}

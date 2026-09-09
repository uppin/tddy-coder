//! The daemon's model registry: provider-backed chat models, registry assistants, and the ACP
//! bridge that lets one be addressed as an agent.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 2. It was **the cleanest extraction in that
//! crate** and was chosen as an early node for exactly that reason: already directory-shaped as
//! `model_registry/`, **zero** outbound `crate::` edges beyond its own directory, and **zero**
//! inline `#[cfg(test)]` lines — all 4,618 lines of its tests are integration tests in dedicated
//! files, so they move with the code rather than being rewritten.
//!
//! Its two proto services, `models.ModelRegistryService` and `acp.AcpService`, were **already their
//! own protos**. Nothing about the wire changes here and no client migrates; only the crate the
//! implementation lives in.
//!
//! `sqlx`, `agent-client-protocol` and `tddy-acp` are attributable to this subsystem alone and
//! leave `tddy-daemon` with it.

use std::path::PathBuf;
use std::sync::Arc;

/// Resolve a session token to the workspace roots a chat may read.
///
/// The daemon's wiring layer builds this over its configured user mapping, which is why it is an
/// alias over a closure rather than a trait.
pub type ChatWorkspaceRoots = Arc<dyn Fn(&str) -> Vec<PathBuf> + Send + Sync>;

/// The registry's durable store — assistants, providers and their credentials.
///
/// Opening it is fallible and the failure is not swallowed: a daemon whose registry is unreadable
/// cannot honestly say what it can attach, and a partial answer reads as "no agents exist" rather
/// than "one source is broken".
#[derive(Debug)]
pub struct ModelRegistryStore {
    // TODO(model-telegram-screen): implement
}

impl ModelRegistryStore {
    /// Open the store at `path`, running any pending migration.
    pub async fn open(_path: &std::path::Path) -> Result<Self, ModelRegistryError> {
        // TODO(model-telegram-screen): implement
        unimplemented!("ModelRegistryStore::open")
    }

    /// Every assistant this daemon can resolve a name against.
    pub async fn assistants(&self) -> Result<Vec<AssistantRow>, ModelRegistryError> {
        // TODO(model-telegram-screen): implement
        unimplemented!("ModelRegistryStore::assistants")
    }
}

/// One registry assistant, as a name resolution sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistantRow {
    pub name: String,
    pub label: String,
    pub model: String,
}

/// Why the registry could not answer.
#[derive(Debug, thiserror::Error)]
pub enum ModelRegistryError {
    #[error("the model registry at {path} could not be opened: {reason}")]
    Unreadable { path: String, reason: String },
}

/// The `models.ModelRegistryService` entry the daemon's wiring layer registers.
///
/// A subsystem crate's whole contract with the wiring layer is to produce a
/// [`tddy_rpc::ServiceEntry`]; nothing else about the daemon's assembly needs to know this crate
/// exists.
pub fn build_model_registry_entry(
    _store: Arc<ModelRegistryStore>,
    _workspace_roots: ChatWorkspaceRoots,
) -> tddy_rpc::ServiceEntry {
    // TODO(model-telegram-screen): implement
    unimplemented!("build_model_registry_entry")
}

/// The `acp.AcpService` entry — a registry assistant addressed as an ACP agent.
pub fn build_model_acp_entry(_store: Arc<ModelRegistryStore>) -> tddy_rpc::ServiceEntry {
    // TODO(model-telegram-screen): implement
    unimplemented!("build_model_acp_entry")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_service_the_wiring_layer_registers() {
        // Given a store and the roots a chat may read
        let store = Arc::new(ModelRegistryStore {});
        let roots: ChatWorkspaceRoots = Arc::new(|_| Vec::new());

        // When
        let entry = build_model_registry_entry(store, roots);

        // Then
        assert_eq!(entry.name, "models.ModelRegistryService");
    }

    #[test]
    fn names_the_acp_service_the_wiring_layer_registers() {
        // Given
        let store = Arc::new(ModelRegistryStore {});

        // When
        let entry = build_model_acp_entry(store);

        // Then
        assert_eq!(entry.name, "acp.AcpService");
    }

    /// A daemon whose registry is unreadable must say so rather than answer "no assistants": an
    /// empty list is a fabricated answer an operator acts on.
    #[tokio::test]
    async fn refuses_to_open_a_registry_it_cannot_read() {
        // Given a path that is a directory, not a database
        let root = tempfile::tempdir().unwrap();

        // When
        let outcome = ModelRegistryStore::open(root.path()).await;

        // Then
        assert!(
            matches!(outcome, Err(ModelRegistryError::Unreadable { .. })),
            "an unreadable registry is an error, never an empty answer"
        );
    }
}

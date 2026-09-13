//! Configuration for TddyServiceGenerator.

/// Configurable code generator for transport-agnostic RPC services.
///
/// Generates service traits (tonic-mirrored signatures) and optionally
/// RpcService server structs with per-method handlers.
#[derive(Debug, Clone)]
pub struct TddyServiceGenerator {
    /// Generate the RpcService server struct with per-method handler structs.
    pub generate_rpc_server: bool,
    /// Generate the tonic adapter that wraps an implementation of the generated trait.
    pub generate_tonic_adapter: bool,
    /// Crate path for RPC types (e.g. `"tddy_rpc"`).
    pub rpc_crate_path: String,
    /// Module path of the tonic server trait the adapter should implement, as tonic-build emitted
    /// it — e.g. `"crate::tonic_worktree::worktree_service_server"`.
    ///
    /// `None` means the `.proto` has no tonic-build pass, so there is no server trait to implement:
    /// the adapter is then emitted as the wrapper struct alone. Set it, and the adapter gains a full
    /// delegating impl of that trait.
    ///
    /// The path's final segment must be tonic-build's own `<service>_server` module name, because
    /// the generated impl refers to the trait through it.
    pub tonic_trait_path: Option<String>,
}

impl Default for TddyServiceGenerator {
    fn default() -> Self {
        Self {
            generate_rpc_server: false,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
            tonic_trait_path: None,
        }
    }
}

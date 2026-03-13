//! Configuration for TddyServiceGenerator.

/// Configurable code generator for transport-agnostic RPC services.
///
/// Generates service traits (tonic-mirrored signatures) and optionally
/// RpcService server structs with per-method handlers.
#[derive(Debug, Clone)]
pub struct TddyServiceGenerator {
    /// Generate the RpcService server struct with per-method handler structs.
    pub generate_rpc_server: bool,
    /// Generate tonic adapter struct (requires `tonic` feature).
    #[allow(dead_code)]
    pub generate_tonic_adapter: bool,
    /// Crate path for RPC types (e.g. `"tddy_rpc"`).
    pub rpc_crate_path: String,
}

impl Default for TddyServiceGenerator {
    fn default() -> Self {
        Self {
            generate_rpc_server: false,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }
    }
}

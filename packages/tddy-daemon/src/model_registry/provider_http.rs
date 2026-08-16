//! The transport budget every provider client runs under.
//!
//! A provider is a third-party endpoint on someone else's machine: it can accept a connection and
//! then say nothing at all. Without a deadline that hangs the RPC that asked — and a LiveKit-routed
//! RPC that never returns never errors either, so the operator sees a spinner rather than a
//! failure. Every request therefore carries a connect and an overall timeout, and a multi-request
//! enumeration carries a budget for the whole walk.

use std::time::Duration;

use super::error::ModelRegistryError;

/// How long a provider has to accept the connection.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// How long one provider request has to complete, connection included.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a whole catalog enumeration has. Ollama's enumeration is one `/api/tags`, one
/// `/api/ps` and then one `/api/show` **per model**, so a host with a large library costs many
/// round trips inside a single `RefreshProviderModels`; the budget bounds the RPC regardless of
/// how many that turns out to be.
pub const ENUMERATION_BUDGET: Duration = Duration::from_secs(120);

/// The deadlines a provider client talks under.
#[derive(Debug, Clone, Copy)]
pub struct ProviderHttp {
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub enumeration_budget: Duration,
}

impl Default for ProviderHttp {
    fn default() -> Self {
        Self {
            connect_timeout: CONNECT_TIMEOUT,
            request_timeout: REQUEST_TIMEOUT,
            enumeration_budget: ENUMERATION_BUDGET,
        }
    }
}

impl ProviderHttp {
    /// An HTTP client that gives up rather than waiting forever.
    ///
    /// `expect` matches `reqwest::Client::new`'s own contract: the build fails only when the TLS
    /// backend cannot be initialized, which no request in this process could survive either.
    pub fn client(&self) -> reqwest::Client {
        reqwest::Client::builder()
            .connect_timeout(self.connect_timeout)
            .timeout(self.request_timeout)
            .build()
            .expect("a provider http client builds unless the TLS backend cannot initialize")
    }

    /// Run `enumeration` under [`Self::enumeration_budget`], reporting a provider that blew it as
    /// a provider failure rather than letting the RPC hang.
    pub async fn within_enumeration_budget<T>(
        &self,
        endpoint: &str,
        enumeration: impl std::future::Future<Output = Result<T, ModelRegistryError>>,
    ) -> Result<T, ModelRegistryError> {
        match tokio::time::timeout(self.enumeration_budget, enumeration).await {
            Ok(result) => result,
            Err(_) => Err(ModelRegistryError::Provider(format!(
                "{endpoint}: enumerating the catalog took longer than {:?}",
                self.enumeration_budget
            ))),
        }
    }
}

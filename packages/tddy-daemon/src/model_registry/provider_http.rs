//! The transport budget every provider client runs under.
//!
//! A provider is a third-party endpoint on someone else's machine: it can accept a connection and
//! then say nothing at all. Without a deadline that hangs the RPC that asked — and a LiveKit-routed
//! RPC that never returns never errors either, so the operator sees a spinner rather than a
//! failure. Every request therefore carries a connect and an overall timeout, and a multi-request
//! enumeration carries a budget for the whole walk.

use std::time::Duration;

use super::error::{truncate_provider_detail, ModelRegistryError};

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

/// Read a provider response, turning a non-2xx status or an unparseable body into a
/// [`ModelRegistryError::Provider`] naming the endpoint — never into a default value.
///
/// Lives here rather than beside one client because every provider client needs exactly this: a
/// second copy is how the two drifted, with only one of them truncating a hostile error page.
pub async fn decode<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
    url: &str,
) -> Result<T, ModelRegistryError> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| ModelRegistryError::Provider(format!("{url}: reading the response: {e}")))?;
    if !status.is_success() {
        // The body is the provider's, not ours: an error page can be hundreds of kilobytes, and
        // this message is persisted on the provider row and returned by every `ListProviders`.
        return Err(ModelRegistryError::Provider(format!(
            "{url}: HTTP {}: {}",
            status.as_u16(),
            truncate_provider_detail(&body)
        )));
    }
    serde_json::from_str(&body)
        .map_err(|e| ModelRegistryError::Provider(format!("{url}: unexpected response ({e})")))
}

/// A provider endpoint that could not be reached at all (DNS, connect, or the transport budget
/// above running out).
pub fn unreachable(url: &str, error: reqwest::Error) -> ModelRegistryError {
    ModelRegistryError::Provider(format!("{error}: {url}"))
}

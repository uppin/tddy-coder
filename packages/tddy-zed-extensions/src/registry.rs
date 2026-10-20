//! Zed's extension registry (R4).

/// The registry host tddy downloads extension archives from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registry {
    api: String,
}

impl Registry {
    /// Zed's own client API host.
    pub const DEFAULT_API: &'static str = "https://api.zed.dev";

    /// A registry at `api` (scheme + host, no trailing path), e.g. a test's fixture server.
    pub fn at(api: impl Into<String>) -> Self {
        Self { api: api.into() }
    }

    /// `TDDY_ZED_EXTENSIONS_API`, else [`Self::DEFAULT_API`]. Only the CLI calls this.
    pub fn from_env() -> Self {
        let api = std::env::var("TDDY_ZED_EXTENSIONS_API")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| Self::DEFAULT_API.to_string());
        Self::at(api)
    }

    /// The API host this registry addresses.
    pub fn api(&self) -> &str {
        &self.api
    }
}

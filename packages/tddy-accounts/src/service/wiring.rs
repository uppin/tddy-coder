use crate::attempts::Attempts;
use crate::linking::{AccountLinker, LinkedAccountStore};
use crate::provider_account::{
    ProviderAccountDefaults, ProviderAccountStore, ProviderModelProbe, RouteSupport,
};
use crate::service::AccountsServiceImpl;
use crate::service::Clock;
use crate::service::Linking;
use crate::service::ProviderAccountPorts;
use crate::sync_status::SyncStatusSource;
use std::sync::Arc;

impl<S> AccountsServiceImpl<S> {
    /// Wire `#keyring` 6/9's sync standing into `AccountSummary.sync_status`. Optional and
    /// additive — every existing call site that skips this keeps reporting
    /// `SYNC_STATUS_UNSPECIFIED`, unchanged.
    #[must_use]
    pub fn with_sync_status(mut self, source: Arc<dyn SyncStatusSource>) -> Self {
        self.sync_status = Some(source);
        self
    }

    /// Wire the half that adds an account: a provider's authorization dance, and somewhere to put
    /// what it yields.
    ///
    /// Additive rather than a second parameter on [`new`](Self::new), so a daemon that only shows
    /// and curates is unchanged. **Neither port can mint a session**: [`AccountLinker`] has no
    /// method that produces one, and [`LinkedAccountStore`] writes records. That is the boundary
    /// `#keyring` 8/9 is built around, expressed as what the types make unreachable.
    #[must_use]
    pub fn with_linking(
        mut self,
        linker: Arc<dyn AccountLinker>,
        store: Arc<dyn LinkedAccountStore>,
    ) -> Self {
        self.linking = Some(Linking {
            linker,
            store,
            attempts: Attempts::default(),
        });
        self
    }

    /// Wire the half that connects model-provider accounts by key.
    ///
    /// Additive, like [`with_linking`](Self::with_linking): a daemon that skips it answers the two
    /// provider-account methods with `FAILED_PRECONDITION` and nothing else changes.
    #[must_use]
    pub fn with_provider_accounts(
        mut self,
        store: Arc<dyn ProviderAccountStore>,
        probe: Arc<dyn ProviderModelProbe>,
    ) -> Self {
        self.provider_accounts = Some(ProviderAccountPorts { store, probe });
        self
    }

    /// Wire the route matrix `ListProviderRoutes` marks each account with. Unwired, that method
    /// answers `FAILED_PRECONDITION`.
    #[must_use]
    pub fn with_route_support(mut self, support: Arc<dyn RouteSupport>) -> Self {
        self.route_support = Some(support);
        self
    }

    /// Wire where per-harness defaults are kept. Unwired, `SetProviderAccountDefaults` answers
    /// `FAILED_PRECONDITION`. Validation uses the route matrix (`with_route_support`).
    #[must_use]
    pub fn with_provider_account_defaults(
        mut self,
        defaults: Arc<dyn ProviderAccountDefaults>,
    ) -> Self {
        self.defaults = Some(defaults);
        self
    }

    /// Replace the clock attempt deadlines and poll pacing are read from. For tests, which would
    /// otherwise sleep to cross a window or an interval.
    #[must_use]
    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }
}

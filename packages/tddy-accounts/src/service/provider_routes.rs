use crate::provider_account::{ProviderAccount, RouteSupport, WireProtocol};
use tddy_service::proto::accounts::ProviderRouteOption;

/// One account as `ListProviderRoutes` offers it for `harness`: the matrix's verdict verbatim, or the
/// harness's own refusal when no account can run it.
pub(super) fn route_option(
    matrix: &dyn RouteSupport,
    harness: &str,
    harness_reason: Option<&str>,
    account: ProviderAccount,
) -> ProviderRouteOption {
    let protocols: Vec<&str> = account
        .wire_protocols()
        .into_iter()
        .map(WireProtocol::as_str)
        .collect();
    let verdict = match harness_reason {
        Some(reason) => Err(reason.to_string()),
        None => matrix.supports(harness, account.preset.id, &protocols),
    };
    ProviderRouteOption {
        account_id: account.account.as_str().to_string(),
        label: account.label,
        preset: account.preset.id.to_string(),
        supported: verdict.is_ok(),
        reason: verdict.err().unwrap_or_default(),
    }
}

# 2026-10-21 — HarnessRouteSupport wired into the accounts service

**Type:** Feature

`#provider-route` 2/9 ([#663](https://github.com/uppin/tddy-coder/pull/663)). New `provider_route_support::HarnessRouteSupport`, the `tddy_accounts::RouteSupport` port over
`tddy_core::provider_route::route_supported` (an id that names no harness is refused as
`unknown harness`). `account_linking::accounts_service_over` wires it with
`with_route_support`, so `ListProviderRoutes` is served wherever the accounts service is. See
[daemon endpoint](../daemon-endpoint.md).

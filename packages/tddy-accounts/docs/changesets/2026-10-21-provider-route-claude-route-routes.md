# 2026-10-21 — ListProviderRoutes behind the RouteSupport port

**Type:** Feature

`#provider-route` 2/9 ([#663](https://github.com/uppin/tddy-coder/pull/663)). New `RouteSupport` port (`harness_routable`, `supports`) and
`AccountsServiceImpl::with_route_support`, which serves `ListProviderRoutes`: the caller's provider
accounts in vault order, each marked with the matrix's verdict verbatim, or every one marked with the
harness's reason when it is not routable at all. Unwired: `FAILED_PRECONDITION`. This crate gains no
dependency on the harness crates. See [provider accounts](../provider-accounts.md#service).

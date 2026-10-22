# 2026-10-22 — Starting a session from a harness profile

**Type:** Feature

`#provider-route` 8/9 ([#669](https://github.com/uppin/tddy-coder/pull/669)). `start_session_core` is
now a wrapper (`harness_profile_request.rs`) that applies the profile named by `req.agent` before the
allowed-agents check and peer forwarding, then calls the former body, renamed `start_session_on_request`.
`harness_profile_start::profile_start` maps a harness to a session type or tool-session agent.
`provider_account_lookup::find_provider_account` backs `SessionAccountAccess::provider_account`: id,
then unique label; `SessionIdentityRefusal::AmbiguousLabel` for a shared label. See
[module layout](../module-layout.md#session-start).

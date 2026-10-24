# 2026-10-24 — A harness profile with no `provider_account` is refused instead of running on the vendor

**Category:** Bug
**Source:** 2026-10-24-project-home-from-new-session-form

- `packages/tddy-discovery/src/harness_profile.rs` documents an empty (or omitted)
  `provider_account` as "the harness on its own vendor", and such a profile loads and is listed.
- `profile_start` (`packages/tddy-agent-launch/src/harness_profile_start.rs`) always answers
  `choice: RouteChoice::Account(profile.provider_account.clone())`, so an empty account becomes
  `RouteChoice::Account("")`. `request_on_profile` (`harness_profile_request.rs`) places it at the
  `agent_definition` level, `choose_route` picks it, and `access.provider_account(&chosen)` refuses
  the start ("no credential vault…"). Every start through such a profile is refused, main included
  (main applies the profile named by its spec's `agent` before its type check).
- **Why deferred:** found outside this change's scope while wiring main's spec through
  `apply_harness_profile`; the earlier project-home path skipped `request_on_profile` for an empty
  account, which hid it. Fixing it changes the route choice for every profile start, which is
  provider-route work, not project-home work.
- **What would close it** (follow-up PR: fix in `harness_profile_start.rs`): make `profile_start`
  answer no account choice (vendor) for an empty `provider_account`, so `choose_route` falls through
  to the request's own choices or the vendor, with unit cases in
  `packages/tddy-agent-launch/src/harness_profile_start_tests.rs` and
  `harness_profile_request_tests.rs` asserting an empty-account profile starts with an empty
  `provider_account_id`.

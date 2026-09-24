# 2026-09-24 — The GitHub device flow, and a testable RealGitHubProvider

**Type:** Feature

`#keyring` 2/9, PR [#509](https://github.com/uppin/tddy-coder/pull/509). Cross-package entry: [`docs/dev/changesets/2026-09-24-keyring-desktop-login.md`](../../../../docs/dev/changesets/2026-09-24-keyring-desktop-login.md)

`GitHubOAuthProvider` gains `start_device_login` / `poll_device_login` (`DeviceLoginStart`, `DeviceLoginPoll { Pending, SlowDown, Denied, Expired, Complete }`), and `authorize_url` returns `Result`. `RealGitHubProvider` builds every request from `oauth_base_url` / `api_base_url` (defaults `GITHUB_OAUTH_BASE_URL` / `GITHUB_API_BASE_URL`; `new_with_base_urls`, `new_public`, `new_public_with_base_urls`), holds the redirect half as one `Option<RedirectClient>` so a public client has no secret and no callback and refuses the redirect flow, POSTs through one `post_for_json` worded by `PostFailures`, and implements the device flow with a bounded, pruned `DeviceAttempt` window and RFC 8628 `slow_down` widening. No device-flow request carries a client secret. `StubGitHubProvider` completes a device login after one pending poll and gains `register_code_mappings`. `AuthServiceImpl` serves `StartDeviceLogin` / `PollDeviceLogin`, finishes both flows through one `complete_login`, and asks `LoginAdmission::admit(github_login, transport)` first. `axum` is a dev-dependency, to serve GitHub on loopback in `tests/real_provider_over_http.rs`.

Closed code issue `missing-tests-real-exchange-code` (claimed by #509): `exchange_code` 71 → 26 lines, 2 → 0 hardcoded hosts, 0 → all 7 original error returns exercised (plus the public-client refusal), 0 → 5 test functions entering it. Detail: [device-flow.md](../device-flow.md).

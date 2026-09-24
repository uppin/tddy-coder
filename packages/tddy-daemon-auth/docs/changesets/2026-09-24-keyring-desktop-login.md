# 2026-09-24 — A client id alone serves the device flow; a desktop enrols its first login

**Type:** Feature

`#keyring` 2/9, PR [#509](https://github.com/uppin/tddy-coder/pull/509). Cross-package entry: [`docs/dev/changesets/2026-09-24-keyring-desktop-login.md`](../../../../docs/dev/changesets/2026-09-24-keyring-desktop-login.md)

`github_provider_kind` is the one decision behind both `build_auth_entries_admitting` and `github_auth_flow`: `stub: true` → the stub; `client_id` + `client_secret` → a confidential `RealGitHubProvider` (redirect flow); `client_id` alone → `RealGitHubProvider::new_public` (device flow); neither → no auth entry. `GitHubAuthFlow { Redirect, Device }` is what both client-config paths declare. A public client refuses `GetAuthUrl` and `ExchangeCode` with `failed_precondition`. `build_auth_entries_with` is `build_auth_entries_admitting(.., None)`. `first_login_admission.rs` adds `FirstLoginEnrolment`: an exhaustive transport match where only `InProcess` enrols, any other transport on an unenrolled desktop is admitted unmapped and writes nothing, a second account is admitted unmapped, and `ConfigNotWritable` refuses `failed_precondition`. `register_stub_codes` is replaced by `StubGitHubProvider::register_code_mappings`. Pinned by `tests/device_login_acceptance.rs` and the inline admission tests.

Code issue moved: `complexity-auth-build-auth-entries-with` → `complexity-auth-build-auth-entries-admitting`, 92 → 106 lines, regressed, deferred with consent. Detail: [auth-service.md](../auth-service.md).

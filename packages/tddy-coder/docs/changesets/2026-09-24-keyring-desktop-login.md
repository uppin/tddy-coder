# 2026-09-24 — The standalone server declares the sign-in flow it registers

**Type:** Feature

`#keyring` 2/9, PR [#509](https://github.com/uppin/tddy-coder/pull/509). Cross-package entry: [`docs/dev/changesets/2026-09-24-keyring-desktop-login.md`](../../../../docs/dev/changesets/2026-09-24-keyring-desktop-login.md)

`StandaloneAuthProvider { Stub, Confidential }` and `standalone_auth_provider(args)` are the one decision `build_auth_service_entry` and `build_client_config` both read, so `auth_flow` at `/api/config` is `"redirect"` exactly when an auth entry is registered and absent otherwise. The standalone server has no public/device provider: a client id without a secret registers nothing. `auth_callback_on` and `auth_service_entry_for` factor the two arms; stub codes register through `StubGitHubProvider::register_code_mappings`. Pinned by `run::standalone_auth_flow_declaration_tests`.

Closed code issue `complexity-run-build-auth-service-entry`: `build_auth_service_entry` 65 lines, nesting 6, 8 branch lines, 0 early exits → 36 lines, nesting 4, 5 branch lines, 1 early exit, 1 parameter.

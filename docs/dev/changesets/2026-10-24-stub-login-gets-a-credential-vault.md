# 2026-10-24 — A stub login gets a credential vault, so `./web-dev` prompts for a passphrase

**Type:** Fix · PR [#711](https://github.com/uppin/tddy-coder/pull/711)

On `./web-dev` (`github.stub: true`) the Accounts screen and the create-session form showed
`[failed_precondition] no credential vault exists for you on this daemon yet; choosing a passphrase
creates one`, and no prompt ever came. The auth service kept stub logins out of the vault entirely
(`VaultState::NONE`, `UnlockVault` refused), while `tddy-accounts` read the same vaults and refused
every provider-account RPC. Desktop, signed in to real GitHub, prompted right after login.

A stub login now gets a vault like a real one; only its synthetic GitHub token is never retained, and
PR lookups in stub mode stay empty (`github.stub` decides them). Accepted relaxation: on a stub
daemon, choosing the passphrase is gated by a stub sign-in — stub mode is local-only.

- `tddy-credentials` — `SessionVaults::sign_in_without_credential` and a credential-free pending
  mark: [entry](../../../packages/tddy-credentials/docs/changesets/2026-10-24-stub-login-gets-a-credential-vault.md)
- `tddy-github` — the auth service keeps vaults for a stub provider:
  [entry](../../../packages/tddy-github/docs/changesets/2026-10-24-stub-login-gets-a-credential-vault.md)
- `tddy-daemon-auth` — acceptance tests for the stub's vault:
  [entry](../../../packages/tddy-daemon-auth/docs/changesets/2026-10-24-stub-login-gets-a-credential-vault.md)
- `dev.daemon.yaml` / `dev.desktop.yaml` — the `pending_login_ttl_seconds` comment says a stub
  sign-in waits that long too.

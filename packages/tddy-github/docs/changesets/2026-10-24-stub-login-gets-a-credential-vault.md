# 2026-10-24 — A stub login gets a credential vault

**Type:** Fix · PR [#711](https://github.com/uppin/tddy-coder/pull/711)
Cross-package entry: [`docs/dev/changesets/2026-10-24-stub-login-gets-a-credential-vault.md`](../../../../docs/dev/changesets/2026-10-24-stub-login-gets-a-credential-vault.md)

`AuthServiceImpl` keeps vaults for a stub provider (`retaining_vaults` → `vaults`, no
`issues_usable_access_token` filter): a stub login calls `SessionVaults::sign_in_without_credential`
instead of retaining its synthetic token, reports the vault's real state, and `UnlockVault` /
`ResetVault` serve it. Test: `retains_no_github_token_for_a_stub_login`. Detail:
[device-flow.md](../device-flow.md).

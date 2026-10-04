# 2026-10-04 — The Accounts service and screen over the credential vault

**Type:** Feature

`#keyring` 4/9 — PR [#511](https://github.com/uppin/tddy-coder/pull/511), `feature/keyring/accounts`,
base `master`. Parent `#keyring` 3/9 [#510](https://github.com/uppin/tddy-coder/pull/510) (the
passphrase credential vault), merged; 1/9 [#508](https://github.com/uppin/tddy-coder/pull/508) and
2/9 [#509](https://github.com/uppin/tddy-coder/pull/509) transitively, merged. Dependents: 5/9
`assignments` ([#512](https://github.com/uppin/tddy-coder/pull/512)), 6/9 `sync`
([#513](https://github.com/uppin/tddy-coder/pull/513)), 7/9 `screen-share`
([#514](https://github.com/uppin/tddy-coder/pull/514)), 8/9 `link-github`
([#515](https://github.com/uppin/tddy-coder/pull/515)) and 9/9 `github-identity`
([#516](https://github.com/uppin/tddy-coder/pull/516)).

## Summary

The first surface over the vault 3/9 introduced. `accounts.proto` (`tddy-service`) defines
`AccountsService` — `ListAccounts`, `SetAccountLabel`, `RemoveAccount` — whose `AccountSummary`
carries **no secret field**; a new crate, `tddy-accounts`, serves it over the daemon's
`SessionVaults` through an `AccountStore` port; the daemon registers it when `auth_storage` is set
and exposes the shared vaults read-only as `DaemonRuntime::credential_vaults()`; and `tddy-web` gains
the ninth daemon-scoped screen, `/accounts`, with a `shell-menu-accounts` entry.

Empty, no vault yet (`vault_uninitialized`), locked (`vault_locked`) and unreadable are four distinct
answers from `VaultState` to the DOM; an unknown session token is refused (`Unauthenticated`), a rename
of an unlinked account is `NotFound`. The screen is not capability-gated.

## Where it is documented

- [tddy-accounts — accounts service](../../../packages/tddy-accounts/docs/accounts-service.md)
- [tddy-web — accounts screen](../../../packages/tddy-web/docs/accounts-screen.md)
- [tddy-daemon — daemon endpoint § credential vaults](../../../packages/tddy-daemon/docs/daemon-endpoint.md)
- [Product — accounts screen](../../ft/web/accounts-screen.md), [app shell](../../ft/web/app-shell.md)
- Package entries: `packages/{tddy-accounts,tddy-web,tddy-daemon}/docs/changesets/2026-10-04-keyring-accounts.md`;
  product entry `docs/ft/web/changelog/2026-10-04-keyring-accounts.md`

## Decisions

- **A new crate, not `tddy-credentials`**: the store stays free of proto and RPC, so the crates that
  depend on it (6/9, 7/9) do not pay for them. `tddy-credentials` gains no dependency.
- **A port over 3/9's data types**: `SessionVault` can only be obtained by sealing a real file, so the
  service's contract is tested over an in-memory `AccountStore`; `SessionVaultAccountStore` is the
  production implementation, tested against real vaults in a temp dir.
- **Stub-provider daemon** (developer decision 2026-10-04): a stub login retains no credential, so
  nothing a stub user does opens a vault. `tddy-daemon/tests/accounts_stub_daemon_acceptance.rs`
  opens the stub user's vault through `DaemonRuntime::credential_vaults()` — the runtime's own
  handle, not a second registry — and calls `ListAccounts` through the real roster.
- **`NotFound` for renaming an unlinked account** (developer decision 2026-10-04), rather than
  `Internal`.

## Verification (scoped)

`./test -p tddy-accounts -p tddy-service -p tddy-daemon` 390 passed, 0 failed, 1 ignored at
`d66d182b`; scoped clippy clean; `scripts/generated-code.sh check` clean; `bun test src/routing` 98
passed; Cypress `AccountsScreenAcceptance` 11/11, `ModelsNavAcceptance` 3/3,
`PresenceCapabilityGatingAcceptance` 16/16. The final tree's scoped gate is `/pr-wrap` step 6; the
workspace is CI's.

## Backlog

This changeset claimed **no** `docs/dev/todo/` entry and **no** code-issue record, so the wrap deleted
none. It leaves three, each kept because its work is not done:

- `docs/dev/todo/2026-10-04-keyring-accounts-rename-lost-update.md` — **added**. `set_label`'s
  get-then-put can overwrite a concurrent write to the same record; closing it needs an atomic update
  on `SessionVault`, which is 3/9's surface.
- `docs/dev/todo/2026-10-04-keyring-accounts-screen-long-components.md` — **added**. `AccountRowView`,
  `renderOutcome` and `AccountsAppPage` are over 60 lines; split deferred with the developer's consent
  until after the stack, because #515 edits both files.
- `docs/dev/todo/2026-09-24-keyring-store-deferred-oversized-file-splits.md` — **extended** with this
  PR's growth of `runtime.rs` and `build.rs`; the splits wait for the stack to land, with the
  developer's consent.

## Code issues — final measurements

Re-measured at wrap by production lines to the first `#[cfg(test)]` and brace matching, merge-base
`b42eb558` → `HEAD`. All three regressed and stay open, with a row naming this PR:

| Record | Before → after |
|---|---|
| `packages/tddy-daemon/docs/code-issues/oversized-file-runtime.md` | 1,677 → 1,700 production lines |
| `packages/tddy-daemon/docs/code-issues/complexity-runtime-build.md` | `build` 937 → 950 lines, nesting 5 unchanged |
| `packages/tddy-service/docs/code-issues/oversized-file-build.md` | 709 → 723 production lines (`main` 648 → 662) |

`tddy-accounts` is new and has no records; `tddy-service`'s `complexity-service-acp-on-event` and
`tddy-web`'s three oversized-file records name files this PR did not touch, and are left alone.

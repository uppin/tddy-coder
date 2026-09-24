# 2026-09-24 — A session-gated encrypted credential store replaces the plaintext GitHub token file

**Type:** Architecture

`#keyring` 3/9 — PR [#510](https://github.com/uppin/tddy-coder/pull/510), base `master`. Parents
`#keyring` 1/9 [#508](https://github.com/uppin/tddy-coder/pull/508) (the per-daemon signing key — a
session exists without a LiveKit secret, so unlocking a vault is an ordinary authenticated RPC) and
2/9 [#509](https://github.com/uppin/tddy-coder/pull/509), both merged. Dependents: 4/9 `accounts`
([#511](https://github.com/uppin/tddy-coder/pull/511)), 6/9 `sync`
([#513](https://github.com/uppin/tddy-coder/pull/513)), 7/9 `screen-share`
([#514](https://github.com/uppin/tddy-coder/pull/514)) and 8/9
`link-github` ([#515](https://github.com/uppin/tddy-coder/pull/515)) build on this store.

## Summary

`GitHubTokenStore` — a two-method trait over a `0600` **plaintext** JSON file,
`auth_storage/github-tokens.json` — is replaced by a new crate, `tddy-credentials`: a generic,
provider-extensible credential store, encrypted at rest and openable only by its owner, with their
**vault passphrase** or with an unlock key one of their browser session lineages holds.
`GitHubTokenStore`, `FileGitHubTokenStore` and `github-tokens.json` are deleted — no migration read,
no fallback.

Records are `(provider, account, secret)`, one file per user (`credentials-<hex login>.vault`), so
`cloudflare`, a second GitHub account and screen-sharing secrets can live in the same place later.
A login's GitHub token is a sealed **record**, never key material. Signing in always completes and
reports the vault's state (`OPEN` / `LOCKED` / `UNINITIALIZED` / `NONE`); a closed vault holds the
token in memory until `UnlockVault` opens it. `ResetVault` is the only answer to a forgotten
passphrase: the old file is set aside, never deleted, and there is no daemon-held master key. A
browser's unlock key carries its credentials across a daemon restart without the passphrase.
Pending sign-ins expire after `github.pending_login_ttl_seconds`, and an open vault nothing uses is
closed after `github.open_vault_idle_ttl_seconds`.

Where the end state is documented:

- [session-auth.md](../../ft/daemon/session-auth.md) — § GitHub access-token retention (the vault
  states, unlocking, restart, reset, the fresh-sign-in rule, the trade-off), § Refresh, § Logout,
  § Security / configuration (`pending_login_ttl_seconds`, `open_vault_idle_ttl_seconds`), § Operator
  migration
- [pr-stack-live-status.md](../../ft/coder/pr-stack-live-status.md) — § Authenticated PR status (the
  unavailable reasons while the vault is closed)
- [`tddy-credentials/docs/credential-store.md`](../../../packages/tddy-credentials/docs/credential-store.md) —
  record model, sealed format, key derivation, the registry of open vaults, what the unlock key and
  the passphrase buy, **known limitations**, and **the two retention rules** the deleted trait
  carried (the token stays out of the session token and off every response; a failed write fails
  the login)
- [`tddy-daemon-auth/docs/auth-service.md`](../../../packages/tddy-daemon-auth/docs/auth-service.md) — § Credential vaults: construction, `pending_login_ttl_seconds`, `open_vault_idle_ttl_seconds`, the sweep, `retained_github_token`
- [`tddy-github/docs/device-flow.md`](../../../packages/tddy-github/docs/device-flow.md) — retention in `complete_login`, § The credential vault's half of `AuthServiceImpl`
- [`tddy-daemon-kernel/docs/daemon-kernel.md`](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md) — `PendingLoginTtl`, `OpenVaultIdleTtl`
- [`tddy-daemon/docs/daemon-endpoint.md`](../../../packages/tddy-daemon/docs/daemon-endpoint.md) — the vaults' injection and the sweep in `runtime::build`
- [`tddy-web/docs/daemon-sign-in.md`](../../../packages/tddy-web/docs/daemon-sign-in.md) — § The credential vault: the unlock key, `CredentialVaultPrompt`
- `tddy-session-lifecycle/docs/session-service.md`, `tddy-daemon-rpc/docs/architecture.md` — `credential_vaults` on the host and the PR-stack handler
- `tddy-host-service/docs/host-registry.md`, `tddy-model-registry/docs/model-registry.md`, `tddy-session-store/docs/architecture.md` — references to the deleted store corrected

| Package | Change |
|---|---|
| `tddy-credentials` (new) | `ProviderId`, `AccountId`, `CredentialRecord`; `CredentialStore::{path_in, create, open_with_passphrase, open_with_unlock_key, reset}`; `SessionVault`; `UnlockKey`; `SessionVaults` + `VaultState`, `Retained`, `Reset`, `ROTATION_GRACE`, pending sign-ins (`sessions/pending.rs`), idle open vaults (`sessions/open.rs`); `SecretString`, `SecretBytes`, `MIN_PASSPHRASE_CHARS` / `MAX_PASSPHRASE_CHARS`; `VaultError`; `atomic.rs` |
| `tddy-github` | `token_store.rs` deleted; `with_credential_vaults`; retention by vault state; `UnlockVault` / `ResetVault`; unlock-key rotation on refresh, slot removal on logout; `src/auth_service/vault.rs`, `vault/backoff.rs` |
| `tddy-daemon-auth` | `github_token_store.rs` deleted; `vault_lifetimes.rs` (was `pending_logins.rs`); `AuthBuildResult::credential_vaults`; `retained_github_token` |
| `tddy-daemon-kernel` | `pending_login_ttl.rs`, `open_vault_idle_ttl.rs`; `GitHubConfig.{pending_login_ttl_seconds, open_vault_idle_ttl_seconds}`; path dependency on `tddy-github` (for `REFRESH_TOKEN_TTL`) |
| `tddy-daemon` | `runtime::build` injects the vaults and spawns the sweep |
| `tddy-session-lifecycle` | `DaemonSessionHost::{with_credential_vaults, credential_vaults}` |
| `tddy-daemon-rpc` | `PrStackRpcHandler.credential_vaults`; `pr_stack/pr_status.rs` reads through `retained_github_token`; path dependency on `tddy-credentials` |
| `tddy-service` | `auth.proto`: `vault_unlock_key` on five messages, `VaultState` on four, `UnlockVault`, `ResetVault`; redacted `Debug` for the passphrase requests |
| `tddy-web` | the unlock key in `sessionTokenStore`; `CredentialVaultPrompt`; `src/lib/vaultPassphrase.ts`; `src/hooks/authSession.ts` split out of `useAuth.ts` |
| `tddy-session-sync`, `tddy-remote-git-repo` | a tool's `RefreshSessionRequest` presents no unlock key |
| `tddy-host-service`, `tddy-rust-typescript-tests` | a doc comment naming the deleted store; regenerated `auth_pb.ts` |
| config | `daemon.yaml.production` (commented example, wording), `desktop.yaml.production`, `dev.daemon.yaml`, `dev.desktop.yaml` (`pending_login_ttl_seconds: 600`, `open_vault_idle_ttl_seconds: 604800`); `install` |

**Dependencies.** `argon2 0.5` was already in the workspace. **`zeroize` 1.9** — approved by the
developer after the wrap, and already in `Cargo.lock` — replaces the hand-rolled volatile-write
wipe in `SecretBytes`, `SecretString` and the transient buffers, and turns on the RustCrypto wipes:
`poly1305` (named only for its `zeroize` feature), `argon2` with `zeroize`, and `hmac` / `sha2`
moved to 0.13 / 0.11 with `zeroize` — the releases whose HMAC state wipes on drop, both already in
the tree. `hkdf` was not taken (HKDF is a dozen lines over `hmac` + `sha2`). `tddy-credentials`
gains the workspace's `log`.

## Decisions

- **A passphrase, not the login token (replan, 2026-09-23).** The first design derived the key from
  the GitHub access token a login returned, on the premise that an OAuth App's token "does not
  expire". It does not expire, but it is **not stable**: GitHub mints a new token at every code or
  device exchange, so after a restart every fresh login derived a key that did not open the vault
  (validation finding V1). The developer chose a user passphrase as the one stable secret. That
  removed the coupling to `#keyring` 2/9's OAuth App choice and `rewrap`.
- **One vault per user, not per daemon** — a vault opens for one subject only, so one shared file
  would have locked every user after the first out of theirs.
- **The browser holds the unlock key** — the developer rejected "enter the passphrase after every
  restart". Rotation on every refresh, a 30 s grace window for two tabs sharing one key
  (`navigator.locks` was rejected: it needs a secure context, and the dashboard is plain http).
- **Signing in always completes and reports the vault**; a failed write still fails the login.
- **A stub login reports `NONE`**, creates nothing and is handed no key.
- **Only a fresh sign-in may choose a passphrase**, and pending sign-ins expire
  (`pending_login_ttl_seconds`, default 600, `0` never, at most seven days) — the developer's
  answer to the security review's S1 residual (the gate was the pending record, not its age).
- **Behaviour carried from the deleted trait's doc comment**, now in `credential-store.md`: the
  token is kept out of the session token because that token reaches a browser over a plain-http LAN
  origin, and a failed write fails the login ("a session minted without its token is a
  half-login").
- **`GetAuthStatusResponse.vault_state`** beyond the brief: without it a page reloaded while
  `LOCKED` could not know to prompt again.

## Wrap: validation findings and their outcome

The `/pr-wrap` security review found no blocker and seven should-fix findings. All were fixed or
documented: **S1** create/reset need a fresh sign-in, reset refused while open, set-aside capped at
5 and refused rather than deleted; **S2** Argon2 on `spawn_blocking` behind a semaphore of 2 and
per-login backoff; **S3** a refresh that cannot read the vault keeps the browser's key (server and
client); **S4** one `transitions` lock across retain and unlock; **S5** a key-less logout drops its
pending token; **S6**, **S7** the missing security and web tests. Nits: **N1** redacted `Debug` for
the passphrase requests; **N2** the directory fsync error propagates; **N3** 1024-char maximum;
**N4** the web counts code points; **N7**, **N8** fixed; **N5**, **N6**, **N9**, **N10** documented
limitations.

## Restructuring at wrap (developer-approved, behaviour-preserving, own commits)

| File | Before → after |
|---|---|
| `tddy-github/src/auth_service.rs` | 621 → 379 production lines (`auth_service/vault.rs` new, 290); 384 / 433 after the wrap's fixes |
| `tddy-credentials/src/sessions.rs` | 528 → 420 production lines (`sessions/pending.rs` new, 170) |
| `tddy-web/src/hooks/useAuth.ts` | 520 → 413 lines (`src/hooks/authSession.ts` new, 124) |

## Code issues — final measurements

No record was claimed by #510, and none closed. Measured to the first `#[cfg(test)]`,
`origin/master` `35cf2913` → #510:

| Record | Measurement | Action |
|---|---|---|
| `packages/tddy-daemon-auth/docs/code-issues/oversized-file-auth.md` | 576 → 622 | opened at this wrap; deferred with consent |
| `packages/tddy-session-sync/docs/code-issues/oversized-file-attach.md` | 517 → 520 | opened at this wrap; deferred with consent |
| `packages/tddy-service/docs/code-issues/oversized-file-build.md` | 692 → 695 (`main` 634) | opened at this wrap; deferred with consent |
| `packages/tddy-daemon-kernel/docs/code-issues/oversized-file-config.md` | 1,472 → 1,474 | regressed; deferred with consent |
| `packages/tddy-daemon/docs/code-issues/oversized-file-runtime.md` | 1,619 → 1,620 | regressed; deferred with consent |
| `packages/tddy-daemon/docs/code-issues/complexity-runtime-build.md` | `build` 879 → 880 lines | regressed; deferred with consent |
| `packages/tddy-daemon-auth/docs/code-issues/complexity-auth-build-auth-entries-admitting.md` | 106 → 110 lines | regressed; deferred with consent |
| `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-connection-service.md` | total 1,652 → 1,652 | touched, unchanged |

Every other open record in the touched packages names a file this PR did not change, and was left
alone.

## Backlog

- **Added**: `2026-09-23-atomic-file-leaf-crate` (the inlined atomic writer),
  `2026-09-23-credential-vault-open-past-its-last-session` (a lineage that lapses without logging
  out keeps the vault open; narrowed at wrap — a key-less logout now drops its pending token),
  `2026-09-24-credential-vault-cipher-key-schedule-not-wiped` (without `zeroize`),
  `2026-09-24-keyring-store-deferred-oversized-file-splits` (the five files above), and
  `2026-09-19-action-sandbox-acceptance-pty-test-does-not-finish` (found by this node's baseline).
- **Kept, not resolved here**: `2026-07-26-pr-stack-status-polling-and-stack-hygiene` — the
  PR-status read path changed, its polling did not.
- **Resolved here**: `2026-09-23-credential-vault-open-past-its-last-session` — closed after the
  wrap on the developer's pattern from the pending-login work (configurable, logged, swept): an
  open vault nothing has used — no login sealing into it, unlock, refresh or credential read — for
  `github.open_vault_idle_ttl_seconds` (default and maximum the seven-day refresh-token lifetime,
  `0` never) is closed and its data key dropped; the next refresh with an unlock key reopens it, as
  after a restart. The "evict at slot eviction too" candidate was not built: the `MAX_UNLOCK_SLOTS`
  bound evicts only when a slot is added, for the lineage that just used the vault, so it can
  never evict a vault's last slot. The entry's "also held" pending tokens were already closed by
  `pending_login_ttl_seconds`. Entry deleted.
- **Resolved here**: `2026-09-24-credential-vault-cipher-key-schedule-not-wiped` — closed after the
  wrap, with the developer's approval of `zeroize`: every key-holding type in `tddy-credentials`
  and every RustCrypto instance it builds is `ZeroizeOnDrop` (or, for `Hmac`, made only of parts
  that are), proven by bound in `secret.rs`'s tests; the `TODO(keyring)` in `vault/crypto.rs` is
  gone. Entry deleted.

## After the wrap: the two gaps left open, closed

Two commits on #510 after its wrap (`0918ff8c`), each closing a backlog entry above.

- **Key schedules are wiped** (`zeroize`, developer-approved). See **Dependencies** and the
  resolved `…-cipher-key-schedule-not-wiped` entry. `secret.rs`'s wipe tests now prove each holder
  is `ZeroizeOnDrop` by bound — `SecretBytes`, `SecretString`, `ChaCha20Poly1305`, and the SHA-256
  core and block buffer an `Hmac<Sha256>` is made of — plus a live-value wipe of each secret type;
  none reads freed memory.
- **An open vault nothing uses is closed** — `github.open_vault_idle_ttl_seconds`
  (`tddy-daemon-kernel`'s `open_vault_idle_ttl.rs`; default and maximum
  `tddy_github::REFRESH_TOKEN_TTL`, seven days; `0` never), built into `SessionVaults` by
  `tddy-daemon-auth`'s `vault_lifetimes::credential_vaults_in` (renamed from `pending_logins`,
  which now takes the `github:` block), held per handle by `tddy-credentials`' new
  `sessions/open.rs`, and swept by `spawn_credential_sweep` every min(pending, idle, 60 s), each
  kind skipped at `0`. A use is a login sealing into the vault, an unlock/create/reset, a refresh
  reopening or rotating through it, or `retained_github_token`'s read (`SessionVaults::use_open`);
  `get`/`state` — a status poll — are not. Logged: the value at startup (`info`, `warn` at `0`),
  each closing (`info`, login and idle seconds). New suite
  `tddy-daemon-auth/tests/open_vault_idle_expiry_acceptance.rs`; eight new `sessions.rs` tests; six
  kernel parsing tests. `MAX_PENDING_LOGIN_TTL_SECONDS` now reads the same constant.
- **Slot-bound eviction was not added**, by the model: `MAX_UNLOCK_SLOTS` evicts only when a slot is
  added, and the added slot belongs to the lineage that just used the vault, so the bound can never
  evict a vault's last slot.

| File | Production lines, wrap → now |
|---|---|
| `tddy-daemon-kernel/src/config.rs` | 1,474 → 1,476 (the field and its pointer) — `oversized-file-config` regressed, deferred with consent |
| `tddy-daemon/src/runtime.rs` | 1,620 → 1,620 (the sweep line renamed); `build` 880 → 880 |
| `tddy-daemon-auth/src/auth.rs` | 622 → 621; `build_auth_entries_admitting` 110 → 109 |
| `tddy-credentials/src/sessions.rs` | 420 → 452 (`sessions/open.rs` new, 129) — under budget, not split |

Verification, scoped: `./test -p tddy-credentials -p tddy-daemon-auth -p tddy-github -p
tddy-daemon-kernel --no-fail-fast` **454 passed / 0 failed** (credentials 78, daemon-auth 153,
kernel 124, github 99). `cargo check --all-targets` on the eleven touched packages clean;
`tddy-daemon-rpc` `query_branch_resolution_acceptance` 13 / 0; `cargo clippy -p <pkg>
--all-targets -- -D warnings` clean on all eleven; rustfmt clean on the touched files.

## Verification at the wrap

Scoped, on the branch at the wrap. `./test -p tddy-credentials -p tddy-daemon-auth -p tddy-github
-p tddy-daemon-kernel --no-fail-fast`: **428 passed / 0 failed** (credentials 68, daemon-auth 143,
kernel 118, github 99). `tddy-daemon-rpc` `query_branch_resolution_acceptance`,
`rpc_handlers_shape`, `rpc_handlers_acceptance`, `orchestrator_repo_root_resolution_acceptance`:
34 / 0. `tddy-service` `unbundle_service_split`, `auth_passphrase_redaction`: 30 / 0.
`scripts/generated-code.sh check`: in sync. `cargo clippy -p <pkg> --all-targets -- -D warnings`
clean on all eleven touched Rust packages. Web: `bun test src/hooks src/lib src/rpc` 594 / 0;
Cypress CredentialVaultPrompt, DeviceLogin, AuthProviderRefresh, DurableSession 50 / 50.
`tddy-session-lifecycle`'s full suite was not run locally (`action_sandbox_acceptance` does not
finish). Whole-workspace health is CI's.

# Changeset: Session-gated encrypted credential store

**Date**: 2026-09-19
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Stack**: `#keyring` 3/9 · branch `feature/keyring/store` · base `feature/keyring/desktop-login` (#509)

## Affected Packages

- **tddy-credentials** (**new crate**): `docs/credential-store.md` — the record model, the sealed
  file format, the passphrase key derivation, the registry of open vaults and its states
  - `src/vault.rs` split into `vault/{format,crypto,unlock}.rs` (replan step A, behaviour-preserving)
  - `src/atomic.rs` — the owner-only swap-then-rename writer, inlined so the crate no longer depends
    on `tddy-core` (V7; duplication recorded in `docs/dev/todo/2026-09-23-atomic-file-leaf-crate.md`)
- **tddy-github**: `src/token_store.rs` — **deleted**; `src/auth_service.rs` writes through the new
  store
- **tddy-daemon-auth**: [auth-service.md](../../../packages/tddy-daemon-auth/docs/auth-service.md)
  - `src/github_token_store.rs` — **deleted**
  - `src/auth.rs:83-152` — vault construction; the half-login rule extended to "cannot open"
- **tddy-daemon**: [daemon-endpoint.md](../../../packages/tddy-daemon/docs/daemon-endpoint.md)
  - `src/runtime.rs:882` — construction and injection. **The file is not split**
- **tddy-session-lifecycle**: [session-service.md](../../../packages/tddy-session-lifecycle/docs/session-service.md)
  - `src/connection_service/svc_pr_status_for_caller.rs:93` — the one external read, migrated
- **tddy-service** (added at green): `proto/auth.proto` — additive `vault_unlock_key` fields on
  `ExchangeCodeResponse`, `PollDeviceLoginResponse`, `RefreshSessionRequest`,
  `RefreshSessionResponse` and `LogoutRequest`; at the replan, a `VaultState` enum on
  `ExchangeCodeResponse`, `PollDeviceLoginResponse`, `RefreshSessionResponse` and
  `GetAuthStatusResponse`, and **two new RPCs**, `UnlockVault` and `ResetVault`
- **tddy-web** (added at green): `src/rpc/sessionTokenStore.ts`, `src/hooks/useAuth.ts` — the unlock
  key is stored beside the refresh token, presented on refresh, replaced with the rotated one, sent
  on logout, cleared with the tokens; a page load holding one refreshes at once. At the replan:
  `src/components/CredentialVaultPrompt.tsx` (mounted in `src/index.tsx`) — the passphrase prompt
  for a `LOCKED` or `UNINITIALIZED` vault, with a forgot-passphrase reset
- Incidental, one line each: `tddy-remote-git-repo` and `tddy-session-sync` (a tool's
  `RefreshSessionRequest` presents no unlock key), `tddy-host-service` (a doc link to the deleted
  store), `tddy-daemon-kernel` (the `auth_storage` doc comment), `tddy-rust-typescript-tests`
  (regenerated `auth_pb.ts`), `daemon.yaml.production`, `desktop.yaml.production`, `install`
- Backlog entries added at the replan: `docs/dev/todo/2026-09-23-atomic-file-leaf-crate.md`,
  `docs/dev/todo/2026-09-23-credential-vault-open-past-its-last-session.md`

## Related Feature Documentation

- [PRD — Session-gated encrypted credential store](../../ft/daemon/1-WIP/PRD-2026-09-19-keyring-store.md)
- [Cross-daemon session authentication](../../ft/daemon/session-auth.md)
- [PR-stack live status](../../ft/coder/pr-stack-live-status.md)

## Summary

Replaces `GitHubTokenStore` — a two-method trait over a `0600` **plaintext** JSON file — with a
generic, provider-extensible credential store in a new crate, `tddy-credentials`, encrypted at rest
and openable only by its owner: with their **vault passphrase**, or with an unlock key one of their
browser session lineages holds.

**Replanned 2026-09-23.** The first design derived the key from the GitHub access token a login
returned. A GitHub OAuth App mints a new token at every code or device exchange, so after a daemon
restart every fresh login was `Locked` and refused (validation finding V1). The developer chose a
user passphrase as the stable secret: Argon2id over it is now the one key-encryption key a user
holds, and the GitHub token is a sealed record only. Signing in always completes and reports the
vault's state; a closed vault holds the login's token in memory until the passphrase opens it.

Records are `(provider, account, secret)` rather than `login → token`, which is what lets
`cloudflare`, a second GitHub account and screen-sharing secrets live in one place later.
`GitHubTokenStore`, `FileGitHubTokenStore` and `github-tokens.json` are **deleted** — no migration
read, no fallback.

## Background

The surface being replaced is small, which is what makes replacing it outright reasonable rather
than reckless.

`packages/tddy-github/src/token_store.rs:15` is `put(login, access_token)` / `get(login)`. Its only
implementation, `packages/tddy-daemon-auth/src/github_token_store.rs`, is one `0600` JSON file
holding `HashMap<String, String>`, written through `write_atomic_with_mode` and serialised by a
process-wide `static PUT_LOCK: Mutex<()>` because concurrent logins would otherwise clobber.

Its **only reader outside the auth crate** is `svc_pr_status_for_caller.rs:93`; the rest is
construction and plumbing (`auth.rs:83-152`, `runtime.rs:882`).

Three properties make it unextendable: the 2-argument signature has no room for a refresh token, a
provider or an account identity; the secret is plaintext at rest, so `0600` protects it from other
users on the host and from nothing that reads the disk; and its doc comment states two rules the
replacement must keep — the token stays **out** of the session token handed to a browser over a
plain-http LAN origin, and a failed `put` **fails the login**, because "a session minted without its
token is a half-login".

The encryption pattern exists already:
`packages/tddy-screen-sharing/src/screen_sharing_vault.rs` (394 lines) — Argon2id, ChaCha20-Poly1305
with a fresh nonce per item, a `VERIFIER_PLAINTEXT` ciphertext that proves a key without storing it,
`write_atomic_with_mode(…, 0o600)`. Four of its limits are fixed here rather than copied.

## Responsibility

**This node owns where a secret lives and what it takes to read one.**

- the `tddy-credentials` crate: `ProviderId`, `AccountId`, `CredentialRecord`, the sealed file
  format with a versioned KDF header, seal/open, and the session-scoped handle;
- key derivation — the passphrase KEK (Argon2id), the random data key it wraps, the browser unlock
  slots, the verifier, and zeroization;
- the vault's states (`OPEN` / `LOCKED` / `UNINITIALIZED` / `NONE`) on the login, refresh and
  status responses, and the two RPCs that act on them, `UnlockVault` and `ResetVault`, with the
  minimal web prompt that calls them;
- the deletion of `GitHubTokenStore`, `FileGitHubTokenStore` and `github-tokens.json`;
- the half-login rule restated: a login whose token cannot be retained is **reported**, never
  silent; a failed write still fails the login.

## Boundaries

**Owned surface:**

| Symbol | Crate |
|---|---|
| `ProviderId`, `AccountId`, `CredentialRecord` | `tddy-credentials` (new) |
| `CredentialStore` (open / put / get / list / remove) | `tddy-credentials` |
| `SessionVault` — the opened, session-scoped handle | `tddy-credentials` |
| the on-disk format and its versioned header | `tddy-credentials` |
| `SessionVaults`, `VaultState`, `Retained`, `Reset`, `ROTATION_GRACE` — the registry of open vaults | `tddy-credentials` |
| `SecretString`, `MIN_PASSPHRASE_CHARS` | `tddy-credentials` |
| `auth.VaultState`, `UnlockVault`, `ResetVault` | `tddy-service` (`auth.proto`) |
| `CredentialVaultPrompt` | `tddy-web` |

**Explicitly not this node's:**

- **The Accounts RPC service and screen** — `#keyring` 4/9. This node ships storage plus the one
  prompt and the two RPCs a passphrase-keyed vault cannot work without (`UnlockVault`,
  `ResetVault`); listing, linking and unlinking accounts stay 4/9's.
- **Propagating the file between daemons** — `#keyring` 6/9.
- **Screen-sharing as a provider** — `#keyring` 7/9. `screen_sharing_vault.rs` is untouched here.
- **A second GitHub account** — `#keyring` 8/9. The record model has room for one; nothing mints it.
- **Injecting `GITHUB_TOKEN` into sessions** — `#keyring` 9/9.
- Splitting `runtime.rs::build` (806 lines) — recorded, not claimed; no restructure in this stack.

**Two lines this node must not cross:**

1. **No API on the store returns a secret to an RPC response path.** The prose rule in
   `token_store.rs` becomes a property of the type. A live `repo`-scoped credential must never reach
   a browser over a plain-http LAN origin.
2. **No daemon-held master key.** A second way into the vault that does not need a user would make
   the daemon able to read credentials with nobody present — the exact property this node removes.
   The consequence is accepted explicitly under `## Technical Changes` § *the locked case*.

**Crate-boundary choice**: `tddy-credentials` depends on neither `tddy-daemon-auth` nor
`tddy-livekit`, so 6/9 and 7/9 can use it without reaching through auth.
[`heavy-dependency-livekit-peer-forwarding`](../../../packages/tddy-daemon-kernel/docs/code-issues/heavy-dependency-livekit-peer-forwarding.md)
is the measured example of what one misplaced dependency costs a crate's dependents — 14 of them,
6 paying for something they never use.

## Dependencies

**Parent in the line**: `#keyring` 2/9 `desktop-login` — [#509](https://github.com/uppin/tddy-coder/pull/509).

**Real dependency edge**: `#keyring` 1/9 `signing-key` — [#508](https://github.com/uppin/tddy-coder/pull/508).
This node needs a session that exists without a LiveKit secret, because unlocking the vault is an
authenticated RPC of a signed-in session. It does **not** depend on 2/9; both are wave 2, and 2/9 sits ahead of it in the line by the
developer's explicit instruction rather than by the sort (see `## Green wave`).

~~**A soft but real coupling to 2/9**: 2/9 chooses an **OAuth App**, whose user access token does
not expire, which was to make a token-derived key deterministic across logins.~~ **This premise was
wrong** (V1): "does not expire" is not "stable" — an OAuth App issues a *new* token at every
exchange. The replan removes the coupling: the key comes from the user's passphrase, so nothing in
this node depends on how long, or how many, GitHub tokens live.

**Dependents**: 4/9 `accounts`, 6/9 `sync`, 7/9 `screen-share`, 8/9 `link-github` — six transitive.

**New external dependencies: none taken.** `argon2 0.5` is added to `tddy-credentials` at the
replan; it was already in the workspace (`tddy-screen-sharing`, lockfile 0.5.3). `chacha20poly1305 0.10`, `sha2 0.10`,
`hmac 0.12`, `subtle 2.6` and `rand` are already in the workspace, and HKDF-Extract/Expand is a
dozen lines over `hmac` + `sha2`. ⚠ **Two would be tidier and both need CLAUDE.md § ASK approval
before use**: `hkdf 0.12` and `zeroize`. Neither is assumed; the design works without them. The
local crates.io proxy makes new Rust crates cheap, so the cost is the review, not the fetch.

## Draft PR contract

Published in this PR's **second commit**:

**Surface** — `tddy-credentials`, signatures only, bodies `todo!()`:

```rust
pub struct ProviderId(String);
pub struct AccountId(String);
pub struct CredentialRecord { provider, account, label, secret, metadata, updated_at }

pub struct CredentialStore;      // the sealed file
pub const VAULT_FILE: &str = "credentials.vault";
impl CredentialStore {
    pub fn path_in(auth_storage_dir: impl AsRef<Path>) -> PathBuf;
    pub fn open_or_create(path: &Path, ikm: &[u8], subject: &str) -> Result<SessionVault, VaultError>;
}
pub struct SessionVault;         // zeroizes on drop
impl SessionVault {
    pub fn path(&self) -> &Path;
    pub fn put(&self, record: CredentialRecord) -> Result<(), VaultError>;
    pub fn get(&self, provider: &ProviderId, account: &AccountId) -> Result<Option<CredentialRecord>, VaultError>;
    pub fn list(&self, provider: Option<&ProviderId>) -> Result<Vec<CredentialRecord>, VaultError>;
    pub fn remove(&self, provider: &ProviderId, account: &AccountId) -> Result<(), VaultError>;
    pub fn rewrap(&self, ikm: &[u8]) -> Result<(), VaultError>;
}
pub struct SecretBytes([u8; 32]);   // key material, zeroed on drop
pub enum VaultError { Locked, FormatMismatch { expected, found }, Io(String), Crypto }
```

> **Three additions to what this section originally listed**, each forced by writing the tests
> against it:
>
> - **`VAULT_FILE` / `CredentialStore::path_in`.** The vault is the replacement for
>   `github-tokens.json`, and the daemon that writes it, the acceptance tests that seal one, and
>   anyone inspecting a deployment's `auth_storage` all have to agree on one name. Leaving it
>   private would make every caller hard-code the string.
> - **`SecretBytes`.** "`SessionVault` zeroizes on drop" needs a type that *does* the zeroizing, and
>   an assertion needs to be able to name it. Hand-rolled — a volatile write plus a fence — rather
>   than taken from `zeroize`, which is tidier and needs CLAUDE.md § ASK approval first.
> - **`VaultError::Io(String)`, not `Io(std::io::Error)`.** `io::Error` is not `PartialEq`, so the
>   enum could not be compared and every test would have to match loosely instead of asserting an
>   exact value. A `String` also matches the trait being replaced, whose errors were already
>   strings carrying server-side detail for the log rather than for the client.

**Failing tests**

- a record written in one session opens in the next from the same derived key;
- the file's bytes contain no plaintext secret, label or metadata;
- tampering with a record's `label` or `metadata` fails the open (AEAD covers the whole record);
- a changed KDF parameter surfaces as `FormatMismatch`, not a failed decrypt;
- a different input keying material yields `Locked` — and **nothing is re-initialised**;
- `rewrap` succeeds and the vault opens under the new key and not the old;
- key material is zeroized on drop;
- acceptance: a `Locked` vault fails the login, and is **not** replaced by an empty one;
- acceptance: a **stub login seals nothing** — see the measured correction below.

> **Measured correction — the third planned acceptance test is already written.** "PR-stack live
> status resolves `Empty` / `Unavailable(reason)` / `Perform(token)` exactly as before" is pinned
> today by `packages/tddy-daemon-auth/tests/pr_lookup_credentials_acceptance.rs` — five tests over
> `pr_lookup_for_caller(stub_mode, Option<&str>)`, a signature this migration does not touch. Only
> *where* `stored` comes from changes (`svc_pr_status_for_caller.rs:93`). A second copy of those
> assertions would add no coverage; that suite is the regression net, and it must still be green
> when `/green` finishes.
>
> **Measured correction — a stub login must seal nothing, and this node decides that here.**
> `packages/tddy-github/src/stub.rs:91` mints `stub-access-token-{uuid}` fresh on every exchange. A
> vault keyed on the login credential would therefore be `Locked` on a demo daemon's *second*
> login — the exact silent breakage this node exists to make impossible. The rule that already
> covers it is the stub's own: `issues_usable_access_token()` is `false`, a stub retains no token
> today, and so a stub login must open no vault and leave no file. Recorded as an acceptance test
> rather than left to be discovered during `/green`, because the alternative — opening a vault
> unconditionally at login — reads entirely reasonable until the second demo login fails.
>
> ~~**Also owed to `#keyring` 2/9's device flow**: a device login's token rotates the vault key, and
> `rewrap` on every login absorbs it.~~ Superseded at the replan: no login token is key material,
> so there is nothing for a second flow to rotate, and `rewrap` is gone.

⚠ **Not mergeable in that state** — implementation follows in this same PR.

> **Changed at green, by the developer's decisions** (recorded here because they change the
> published surface):
>
> - **One vault per user, not per daemon.** `CredentialStore::path_in(auth_storage, subject)` →
>   `credentials-<hex subject>.vault`; `VAULT_FILE` is gone. A vault opens for one subject only (a
>   test pins that), so the single `credentials.vault` the contract named would have locked every
>   user after the first out of theirs — a regression from `github-tokens.json`, which held many
>   logins. The daemon-auth login acceptance tests now seal the vault at `path_in(&storage,
>   THE_LOGIN)`; their intent — *that user's* locked vault refuses the login and is not replaced —
>   is unchanged.
> - **Multiple wrap slots.** The data key is wrapped by the login slot (`header` +
>   `wrapped_data_key`, rewrapped on every login) **and** by up to `MAX_UNLOCK_SLOTS` (16) unlock
>   slots, one per browser session lineage, beside the header in `unlock_slots`. Beside rather than
>   inside: the header is the login slot's associated data, and a slot added at a refresh — with no
>   login credential present — must not invalidate the wrap only a login can rewrite. The tests
>   also pin `header.salt` as the login slot's salt.
> - **The browser holds the unlock key.** The developer rejected "log in again after a restart".
>   Added surface: `UnlockKey` (`to_wire` / `from_wire`, `<hex subject>.<slot id>.<hex key>`),
>   `CredentialStore::{open_existing, open_with_unlock_key}`,
>   `SessionVault::{add_unlock_slot, rotate_unlock_slot, remove_unlock_slot, unlock_slot_ids}`,
>   `SessionVaults::{new, unlock, reopen, forget, get, path_for}`. The key carries its subject so a
>   logout can remove its slot even with a lapsed access token.
> - **`open_existing`** exists for the stub rule: a stub login never creates a vault, but one that
>   is already there and does not open still refuses it (`Locked`) — the daemon-auth acceptance
>   test uses a stub provider for exactly that.
> - **A refresh whose unlock key does not open its slot still succeeds**, returning an empty key,
>   and is logged at `warn` (`tddy_github::auth_service`). Failing it would sign the operator out
>   of everything for a credential-store problem; PR status then reads *unavailable* until the next
>   login re-issues a key.
> - **`LogoutRequest.vault_unlock_key`** is a fifth field beyond the four the brief listed:
>   without it a logout cannot name the lineage whose slot it removes.

> **Changed at the replan (2026-09-23), by the developer's decision** — a passphrase replaces the
> login-derived key (V1). Published in `73e07aa3`, implemented in `b4715d2a`:
>
> ```rust
> impl CredentialStore {
>     pub fn path_in(auth_storage_dir: impl AsRef<Path>, subject: &str) -> PathBuf;
>     pub fn create(path: &Path, passphrase: &SecretString, subject: &str) -> Result<SessionVault, VaultError>;
>     pub fn open_with_passphrase(path: &Path, passphrase: &SecretString, subject: &str) -> Result<SessionVault, VaultError>;
>     pub fn open_with_unlock_key(path: &Path, unlock: &UnlockKey) -> Result<SessionVault, VaultError>;
>     pub fn reset(path: &Path, new_passphrase: &SecretString, subject: &str) -> Result<(SessionVault, Option<PathBuf>), VaultError>;
> }
> impl SessionVault { /* put, get, list, remove, add_unlock_slot, remove_unlock_slot, unlock_slot_ids */
>     pub fn rotate_unlock_slot(&self, presented: &UnlockKey) -> Result<UnlockKey, VaultError>; // was (&str)
> }
> pub enum VaultState { Open, Locked, Uninitialized }
> impl SessionVaults {
>     pub fn state(&self, subject: &str) -> VaultState;
>     pub fn holds_pending(&self, subject: &str) -> bool;
>     pub fn retain(&self, subject: &str, record: CredentialRecord) -> Result<Retained, VaultError>;
>     pub fn unlock(&self, subject: &str, passphrase: &SecretString) -> Result<UnlockKey, VaultError>;
>     pub fn create(&self, subject: &str, passphrase: &SecretString) -> Result<UnlockKey, VaultError>;
>     pub fn reset(&self, subject: &str, new_passphrase: &SecretString) -> Result<Reset, VaultError>;
>     pub fn with_rotation_grace(self, grace: Duration) -> Self;   // + reopen, forget, get, path_for
> }
> pub struct SecretString;          // CredentialRecord.secret's type; redacted, wiped, no serde
> pub const MIN_PASSPHRASE_CHARS: usize = 8;
> pub const ROTATION_GRACE: Duration = Duration::from_secs(30);
> pub enum VaultError { Locked, Uninitialized, AlreadyInitialized, FormatMismatch { .. }, Io(String), Crypto }
> ```
>
> - **Gone**: `open_or_create`, `open_existing`, `rewrap`, `SessionVaults::unlock(subject, ikm)`.
>   Format version 2; version 1 files (test-only) are refused by name, not migrated.
> - **`auth.proto`**: `VaultState { UNSPECIFIED, NONE, OPEN, LOCKED, UNINITIALIZED }` on the three
>   login/refresh responses the brief named **and on `GetAuthStatusResponse`** — a deviation: without
>   it a page reloaded while `LOCKED` (no unlock key, so no page-load refresh) could not know to prompt
>   again. `UnlockVault(session_token, passphrase, create) → { vault_state, vault_unlock_key }` and
>   `ResetVault(session_token, new_passphrase) → { vault_state, vault_unlock_key }`.
> - **`unbundle_service_split` needed no registration.** Its residual-methods closed-world list
>   covers the four protos `connection.ConnectionService` was split into (session, project,
>   demo_vm, local_token), not `auth.proto`; the suite passes unchanged (28/28).
> - **Pending tokens are bound to the user, not the lineage**: session tokens are stateless and
>   carry no lineage id, so a daemon cannot tell two lineages of one user apart. A later token for
>   the same account replaces an earlier one.
> - **A stub login reports `NONE`** and no longer opens an existing vault: its token is synthetic and
>   no longer key material, so there is nothing for it to open or be refused by.

## Green wave

**Wave 2 of 5**, with `#keyring` 2/9. Its only unmet need is wave 1.

```
waves   1:{n1}   2:{n2, n3}   3:{n4}   4:{n5, n6, n7, n8}   5:{n9}
line    n1 · n2, n3 · n4 · n5, n6, n7, n8 · n9
```

⚠ **Declared deviation**: the intra-wave sort puts blockers first, and this node has **six**
transitive dependents to 2/9's one — so the sort would place it ahead of 2/9. It sits second by the
developer's explicit instruction that a desktop local-login node be among the first PRs. Recorded so
the order is not mistaken for the sort's output; the cost is one PR of latency for this node and
nothing downstream, because both are wave 2 and neither depends on the other.

## Prerequisites

### ⚠ DURING — `runtime::build` complexity — [`complexity-runtime-build`](../../../packages/tddy-daemon/docs/code-issues/complexity-runtime-build.md)

`runtime.rs:498`, 806 lines, covering the `:882` token-store wiring this node rewires. Recorded,
**not claimed**: two independent backlog entries and this repo's planning policy say a mechanical
split inside a feature PR buries the reviewable diff. Replacing the store's construction makes the
function marginally smaller as a side effect, which is the only size change this node makes.

### ⚠ DURING — PR-stack status polling and stack hygiene — [`2026-07-26-pr-stack-status-polling-and-stack-hygiene.md`](../todo/2026-07-26-pr-stack-status-polling-and-stack-hygiene.md)

`svc_pr_status_for_caller.rs` is the one external reader this node migrates, and the entry is about
that feature's polling. Recorded, not fixed — the read path changes, the polling does not.

### ℹ ANSWERED — the retention rule in `token_store.rs`'s doc comment

Not a backlog entry, but the standing rule the deleted trait carried, recorded here because deleting
the file deletes the only place it is written down. **The GitHub token is kept out of the session
token because that token goes to a browser over a plain-http LAN origin**, and **a failed write
fails the login**. Both are carried into this node's acceptance criteria and into
`tddy-credentials`' own documentation, which is where they live after this change.

⚠ **Also stale after `#keyring` 1/9**: the same comment calls the session token "HMAC". 1/9 owns
that correction; this node deletes the file, so whichever lands first, the sentence does not
survive.

### ⚠ DURING — `action_sandbox_acceptance` does not finish locally — [`2026-09-19-action-sandbox-acceptance-pty-test-does-not-finish.md`](../todo/2026-09-19-action-sandbox-acceptance-pty-test-does-not-finish.md)

Found by this node's own wave-2 baseline: `sandboxed_bash_pty_action_streams_output` in
`tddy-session-lifecycle` ran **over 14 minutes** without a result and had to be killed before the
scoped gate could continue. Because `./test` is single-threaded, it blocks every suite behind it, so
anyone scoping a gate to this package gets no result rather than a failure. Recorded, **not
claimed** — this node touches `svc_pr_status_for_caller.rs`, not the sandbox or the action runner,
and an unrelated hang investigation does not belong in a credential-store diff.

### Unanalyzed packages

`tddy-github` was analyzed on 1/9 (one record, claimed by 2/9). `tddy-session-lifecycle` (22
records), `tddy-daemon` (3) and `tddy-daemon-auth` (1) are analyzed; none of their records outside
the two above is in this node's path. `tddy-credentials` is new, so there is nothing to have
analyzed.

## Scope

- [x] **PRD**: [PRD-2026-09-19-keyring-store.md](../../ft/daemon/1-WIP/PRD-2026-09-19-keyring-store.md)
- [x] **Changeset**: this document
- [x] **Draft PR contract**: owned surface + failing tests (wave 2, commit 2; replan `73e07aa3`)
- [x] **New crate**: `tddy-credentials` — record model, sealed format, session handle, split into
      modules under the 500-line budget (replan step A, `91a8b10d`)
- [x] **Key derivation**: passphrase KEK (Argon2id), wrapped data key, unlock slots, verifier,
      zeroize (replan `b4715d2a`)
- [x] **Vault states and RPCs**: `VaultState`, `UnlockVault`, `ResetVault`, the web prompt
- [ ] **Deletions**: ✅ `token_store.rs`, `github_token_store.rs` and every code/config
      `github-tokens.json` reference; ⚠ the `packages/*/docs` and `docs/ft` references are owed at wrap
- [x] **Migration**: `svc_pr_status_for_caller.rs:93`
- [x] **Dependency approvals**: neither `hkdf` nor `zeroize` taken
- [x] **Testing**: unit + acceptance, scoped (see *Measured state (replan)*)
- [ ] **Package Documentation**: ✅ `tddy-credentials/docs/credential-store.md`; ⚠ the other
      packages' docs are owed at wrap
- [ ] **Code Quality**: ✅ scoped clippy clean on all ten touched packages; ⚠ CI not yet read

## Technical Changes

### State A (Current)

- `GitHubTokenStore` — `put(login, access_token)` / `get(login)`; one implementation over a `0600`
  plaintext JSON `HashMap<String, String>`, serialised by a process-wide `PUT_LOCK`.
- One external reader: `svc_pr_status_for_caller.rs:93`.
- No provider dimension, no account identity beyond a login string, no room for a refresh token.
- A backup or snapshot of the data directory contains live `repo`-scoped GitHub credentials.

### State B (Target)

- `tddy-credentials` holds `(provider, account)`-keyed records whose label, metadata **and** secret
  are one sealed AEAD unit, in a file with a versioned KDF header.
- The data key is random and wrapped under a KEK that Argon2id derives from the user's **vault
  passphrase**, and once more per browser lineage under an unlock key that lineage holds. At rest
  the file holds ciphertext and wrapped keys; nothing in it opens it.
- A login's GitHub token is a record, never key material. Signing in always completes and reports
  the vault's state; a closed vault holds the token in memory until the passphrase opens it.
- `SessionVault` zeroizes on drop, and the registry drops it when its last lineage signs out.
- `github-tokens.json` does not exist and is never read.

### Delta (What's Changing)

#### tddy-credentials (new)
- **Architecture**: depends on neither the auth crate nor LiveKit — deliberately, so 6/9 and 7/9 can
  use it directly — and, after the replan, not on `tddy-core` either (V7).
- **API**: `CredentialStore::{create, open_with_passphrase, open_with_unlock_key, reset}`,
  `SessionVault::{put, get, list, remove, add_unlock_slot, rotate_unlock_slot, remove_unlock_slot}`,
  `SessionVaults::{state, retain, unlock, create, reset, reopen, forget, get}`, `VaultState`,
  `SecretString`, `VaultError::{Locked, Uninitialized, AlreadyInitialized, FormatMismatch, Io, Crypto}`.
- **Implementation**: ChaCha20-Poly1305 per record with the record's identity as associated data;
  Argon2id (m=19456 KiB, t=2, p=1, 16-byte salt) + HKDF-Expand for the passphrase slot, HKDF-SHA256
  for the unlock slots, every `info` labelled; a verifier ciphertext; an owner-only
  swap-then-rename writer (`atomic.rs`).

#### the locked case
`LOCKED` is where a vault file exists and nothing has opened it since this daemon started — after
every restart, for any browser that holds no unlock key. It is **no longer a refused login**: the
operator is signed in, the response says `LOCKED`, the login's GitHub token is held in memory
(never written in plaintext), and the page asks for the passphrase. `UnlockVault` opens the vault,
seals the waiting token, hands the lineage an unlock slot and returns its key. A wrong passphrase is
`failed_precondition` naming `Locked`, and changes nothing on disk. There is still **no second key**
and still **no silent re-initialisation**: a forgotten passphrase is an explicit `ResetVault`, which
renames the old file to `credentials-<hex>.locked-<unix>.vault` — never deletes it — and seals the
waiting token into a fresh vault under the new passphrase.

`UNINITIALIZED` is the same, before the first passphrase: `UnlockVault` with `create` makes the
vault (at least `MIN_PASSPHRASE_CHARS`, 8, characters). While either holds, PR status is
*unavailable* with a reason that says to unlock the credential vault.

**A restart need not ask for the passphrase.** Each lineage that opens the vault holds an unlock
key `U` to a slot of its own; its next refresh presents `U`, the daemon opens the vault through the
slot, rotates it and returns `U'`. The rotation proves `U` under the write lock, and the key a
refresh just retired is answered with the same `U'` for 30 s, so two tabs sharing one stored key
both keep a working one (`navigator.locks` was rejected: it needs a secure context, and the
dashboard is plain http). A logout removes the slot; the last one out drops the open vault. The
trade-off — `U` or the passphrase plus the disk is the plaintext, both cross plain http, and an old
backup keeps its old slots — is stated in `tddy-credentials/docs/credential-store.md`.

#### tddy-github
- **API**: `token_store.rs` deleted. `auth_service.rs` retains a `CredentialRecord` through
  `SessionVaults::retain`, reports `vault_state` on every login, refresh and status response, and
  serves `UnlockVault` / `ResetVault`. `GITHUB_ID_METADATA` / `AVATAR_URL_METADATA` name the record's
  metadata keys for 4/9. A clock before the epoch is an explicit error, not `updated_at = 0` (V13).

#### tddy-daemon-auth
- **Implementation**: `github_token_store.rs` deleted; `auth.rs` constructs the registry over
  `auth_storage`; `github_pr_credentials::retained_github_token` reads by vault state and names the
  remedy ("unlock your credential vault") while it is closed.

#### tddy-daemon
- **Implementation**: `runtime.rs:882` constructs and injects the store. **Not split.**

#### tddy-session-lifecycle
- **Integration**: `svc_pr_status_for_caller.rs:93` reads through `SessionVault`.
  `pr_lookup_for_caller`'s three outcomes are unchanged.

## Implementation Milestones

- [x] **M1** — `tddy-credentials`: record model, versioned header, seal/open, `write_atomic_with_mode`
- [x] **M2** — key derivation, wrapped data key, verifier, `rewrap`, zeroization — plus unlock slots
      (the login-derived KEK and `rewrap` superseded by M11)
- [x] **M3** — wire construction in `auth.rs` and `runtime.rs`; derive on login, re-wrap on login
      (superseded by M11: a login retains its token by vault state);
      unlock key on login, reopen + rotate on refresh, remove on logout
- [x] **M4** — migrate `svc_pr_status_for_caller.rs:93` (through `retained_github_token`)
- [x] **M5** — delete `token_store.rs`, `github_token_store.rs`, every code/config `github-tokens.json`
      reference
- [x] **M6** — extend the half-login rule to `Locked`
- [x] **M7** — `tddy-credentials` documentation, carrying the two retention rules forward
- [x] **M8** (added) — the unlock key over the wire: `auth.proto` fields, `tddy-web` storage/refresh/logout

**Replan (2026-09-23)** — V1 and the should-fix findings:

- [x] **M9** — split `vault.rs` (835 production lines) into `vault.rs` 386, `vault/format.rs` 137,
      `vault/crypto.rs` 160, `vault/unlock.rs` 206; behaviour-preserving (`91a8b10d`)
- [x] **M10** — tests first for the passphrase design, V1–V5 and T1–T6 (`73e07aa3`)
- [x] **M11** — the passphrase KEK, vault states, `UnlockVault` / `ResetVault`, pending tokens, the
      web prompt; V2, V3, V4, V5, V7, V9, V13, V14 (`b4715d2a`)
- [x] **M12** — `credential-store.md` (V6's trade-off restated), this changeset, the PRD

**Implementation status (green).** All milestones done, including the replan's; see
*Measured state (replan)* for the scoped counts. Still owed at wrap, because `packages/*/docs/` moves only through this changeset:
`tddy-daemon-auth/docs/auth-service.md` (lines 15, 96, 114 name `github_token_store`),
`tddy-host-service/docs/host-registry.md:41`, `tddy-model-registry/docs/model-registry.md:41`,
`tddy-session-store/docs/architecture.md:68`, `tddy-github/docs/code-issues/missing-tests-real-exchange-code.md:53`,
and `docs/ft/daemon/session-auth.md` § GitHub access-token retention /
`docs/ft/coder/pr-stack-live-status.md:224,446` — all still describe the deleted plaintext store.
⚠ TODOs left: the cipher/HMAC key-schedule copies are not wiped without `zeroize`
(`vault/crypto.rs`); a vault whose last lineage lapses without a logout stays open
(`sessions.rs` → `docs/dev/todo/2026-09-23-credential-vault-open-past-its-last-session.md`); the
inlined atomic writer (`atomic.rs` → `docs/dev/todo/2026-09-23-atomic-file-leaf-crate.md`).

## Testing Plan

### Testing Strategy

**Two levels, and the split is principled.** The file format and key derivation are pure
input/output over bytes and a temp directory — **unit**, where an assertion can look at the actual
ciphertext. The rules that matter to a user — a failed write fails the login, a locked vault fails
the login, PR status behaves identically — are properties of the wired daemon, so **acceptance**.

### Unit tests (`tddy-credentials`)

- A record written and re-opened with the same passphrase round-trips exactly; a wrong passphrase
  is `Locked` and changes nothing; the same passphrase for another subject is `Locked`.
- A reset sets the old file aside byte-for-byte and only the new passphrase opens the fresh one.
- The registry: a first login is `Uninitialized` and writes nothing; after a restart a login with a
  **new token** is `Locked`, and the passphrase opens the same vault holding the new token (V1); a
  login while open needs no passphrase and gets a slot; a stale handle is dropped (V2); the last
  logout drops the open vault (V3); rotation proves the key under the lock, a retired key within the
  grace window gets the same successor, and two racing refreshes both end with one working key (V4).
- **The file's bytes contain no plaintext secret, label or metadata.** Asserted against the file
  contents, not inferred from the API.
- Flipping a byte in a record's sealed label or metadata fails the open — the AEAD covers the whole
  record, which is the limit being fixed relative to `screen_sharing_vault.rs`.
- A changed KDF name, format version, Argon2 version or cost surfaces as `FormatMismatch`, not as a
  failed decrypt; an edited salt is `Locked`.
- A record moved under another account's id does not open.
- Key material and secret text are zeroized and print redacted.

### Acceptance tests

- **A failed credential write fails the login** — over an open vault whose file can no longer be
  written (`tddy-github`).
- **A fake GitHub mints a new token per exchange** (T1), in `tddy-daemon-auth/tests/support/mod.rs`,
  shared by both vault suites (T8 within the crate).
- **V1**: restart, then a fresh login with a different token → signed in, `LOCKED`; the passphrase
  opens the same vault; PR status performs with the new token.
- First login → `UNINITIALIZED`, create → `OPEN`; wrong passphrase → `failed_precondition` naming
  `Locked`, file unchanged, still signed in and `LOCKED`; reset keeps the old file aside; no
  passphrase in any log line, file or response; a stub login creates nothing and reports `NONE`.
- T2–T6: a real (non-stub) login over a closed vault; a refresh presenting another user's key; a
  refresh whose key no longer opens (still refreshes, `""`, `LOCKED`); refresh and device-login
  responses carry no GitHub token; two tabs refreshing with one key at once.
- **PR-stack live status is behaviour-identical** — Given a stored token, a stub-mode daemon, and a
  daemon with no stored token, Then `Perform(token)`, `Empty` and `Unavailable(reason)` resolve
  exactly as before.

### Verification scope

`./test -p tddy-credentials -p tddy-github -p tddy-daemon-auth -p tddy-daemon -p tddy-session-lifecycle`
and scoped clippy per package. Whole-workspace green comes from CI via `scripts/ci-status.sh`.

### Measured state (replan)

| Run | Command | Result |
|---|---|---|
| Step A baseline and after | `./test -p tddy-credentials` | 30 passed / 0 failed, both |
| Step B (tests first) | `cargo test -p tddy-credentials -p tddy-daemon-auth -p tddy-github --no-fail-fast` | 170 passed / **75 failed**, every failure a stubbed entry point |
| Step B, web | `bun test src/rpc/sessionTokenStore.test.ts` | 10 passed / **10 failed** (store methods not yet written) |
| Step C | `./test -p tddy-credentials -p tddy-daemon-auth -p tddy-github --no-fail-fast` | **245 passed / 0 failed** |
| Step C | `cargo test -p tddy-service --test unbundle_service_split` | 28 / 0 |
| Step C | `cargo test -p tddy-session-lifecycle --test query_branch_resolution_acceptance --test orchestrator_repo_root_resolution_acceptance` | 18 / 0 (the rest of the package not run: `action_sandbox_acceptance` hang) |
| Step C, web | `bun test src/hooks src/lib src/rpc` | 585 / 0 |
| Step C, web | `cypress run --component --spec cypress/component/CredentialVaultPromptAcceptance.cy.tsx` | 10 / 0 |

`cargo check --all-targets` and `cargo clippy --all-targets -- -D warnings` are clean on all ten
touched Rust packages. Whole-workspace health is CI's.

### Measured red state (wave 2)

Scoped to the two packages this commit touches, on this branch, with `--no-fail-fast` because
`./test` is `--test-threads=1` and stops at the first failing binary otherwise.

| Run | Command | Passed | Failed |
|---|---|---|---|
| Baseline, before this commit | `./test -p tddy-github -p tddy-daemon-auth -p tddy-session-lifecycle --no-fail-fast` | `tddy-daemon-auth` 75 | 14 |
| After this commit | `./test -p tddy-credentials -p tddy-daemon-auth --no-fail-fast` | 76 | 29 |

The delta is **exactly this node's 15 new failures**, and one new test already passes:

- **12 of 13** in `tddy-credentials` — 11 reach `CredentialStore::open_or_create`'s `todo!()`;
  `key_material_is_zeroed_when_the_thing_holding_it_is_dropped` fails on its own assertion
  (`[0x5a; 32]` still in the slot), because `Drop for SecretBytes` is deliberately **not** written
  yet — a missing `Drop` fails the assertion cleanly, where a half-written one would panic in drop.
- **1 of 13 passes**: `key_material_does_not_print_itself`. `Debug for SecretBytes` is part of the
  published surface rather than of the implementation, so the redaction holds from this commit on.
- **3 of 3** in `tddy-daemon-auth`'s `login_opens_the_credential_store_acceptance.rs`. Two fail in
  their *Given* on the same `todo!()`. The third,
  `a_stub_login_leaves_no_credential_store_behind`, fails on
  `Err("session token signing is not configured")` — **an inherited reason**: `#keyring` 1/9's
  `DaemonSigningKey::load_or_generate` is itself still `todo!()`. Its own assertion cannot be
  exercised until 1/9 is green, which is a scheduling fact, not a defect in the test.

The **14 inherited failures** are unchanged in count and identity: six `signing_key::tests::*` and
eight acceptance tests across 1/9 and 2/9, every one of them on `DaemonSigningKey::load_or_generate`
or `AuthServiceImpl::start_device_login`. Nothing this commit adds made an inherited failure worse.

`tddy-session-lifecycle` is in the baseline and not in the after-run: this commit does not touch it
yet (the `svc_pr_status_for_caller.rs` migration is green-phase work), and its
`action_sandbox_acceptance` hang — recorded under **Prerequisites** — makes it expensive to re-run
for no signal.

## Acceptance Criteria

- [x] A credential is readable in-session and **unreadable from the file alone** — nor is the
      passphrase; the file plus an unlock key or the passphrase is the plaintext (stated trade-off)
- [x] `label` and `metadata` are inside the AEAD — tampering with either fails the open
- [x] The header carries KDF name, version and parameters; a change is a detected `FormatMismatch`
- [x] **After a restart, a fresh login with a different GitHub token opens the same vault once the
      passphrase is given** (V1)
- [x] A login over a closed vault is signed in and reports `LOCKED` / `UNINITIALIZED`; the token is
      held in memory and sealed on unlock, never written in plaintext
- [x] A wrong passphrase is `failed_precondition` naming `Locked`, the file unchanged, the session
      still signed in — **no re-initialisation and no second key**
- [x] A reset renames the old vault aside (never deletes it) and seals the waiting token into a
      fresh one under the new passphrase
- [x] A login while the vault is open needs no passphrase; the lineage just gets an unlock slot
- [x] A restart plus a refresh with the unlock key needs no passphrase
- [x] A failed write fails the login
- [x] No store API returns a secret to an RPC response path — `CredentialRecord` is not serialisable
      and its secret prints redacted; no passphrase reaches a log, the file or a response
- [ ] Key material is zeroized on drop and not cached beyond a session — ✅ dropped at the last
      logout; ⚠ a lineage that lapses without a logout keeps it until exit (`docs/dev/todo/`)
- [x] A stub login creates nothing and reports `NONE`
- [ ] `GitHubTokenStore`, `FileGitHubTokenStore` and all `github-tokens.json` references are gone —
      ✅ code and config; ⚠ docs owed at wrap
- [x] PR-stack live status behaves identically, and says to unlock the credential vault while it is
      closed

## Validation Results

**Last run**: 2026-09-23, `/pr-wrap` analysis steps (validate-changes → validate-tests →
validate-prod-ready → analyze-clean-code). Diff range `origin/feature/keyring/desktop-login..HEAD`
(5 commits, 47 files). **Overall: ❌ one blocker, not ready to merge.**

### Stack gate

| Check | Result |
|---|---|
| Stack branch | Yes, planned (base `feature/keyring/desktop-login`, PR #510) |
| `/pr-stack-rebase` | ✅ Already current (merge-base = base tip `2aeeff08`) |
| Leak check | ✅ Clean: only this PR's 5 commits |
| Diff contains only this PR's files | ✅ Every file is claimed by an Affected Packages entry |
| Parent-owned files intact | ✅ Only `token_store.rs` and `github_token_store.rs` deleted, both planned (M5) |
| `## Dependencies` not implemented here | ✅ `poll_device_login` sets this PR's own `vault_unlock_key` field. No 1/9 or 2/9 symbol re-implemented |
| `## Boundaries` respected | ✅ No new RPC, `screen_sharing_vault.rs` untouched, `runtime.rs` not split, no `hkdf`/`zeroize` |
| `## Responsibility` delivered | ⚠️ Delivered in code, but the key-derivation premise does not hold against real GitHub (V1) |

### Build (scoped)

| Package | Result |
|---|---|
| tddy-credentials | ✅ `cargo clippy -p tddy-credentials --all-targets -- -D warnings` clean |
| tddy-credentials, tddy-daemon-auth, tddy-github | ✅ 201 passed / 0 failed (scoped run before this validation) |
| tddy-web | ✅ `bun test src/hooks src/lib`: 349 pass |
| tddy-session-lifecycle, tddy-daemon | ✅ `cargo check -p tddy-session-lifecycle -p tddy-daemon` clean, 0 warnings. Suites not run locally (the `action_sandbox_acceptance` hang, see Prerequisites); CI owns them |

Whole-workspace health comes from CI; nothing workspace-wide was run locally.

### Changeset sync

| Item | Recorded | Actual | Updated to |
|---|---|---|---|
| Scope: Draft PR contract, New crate, Key derivation, Migration | 🔲 | Code present (`3e8e6331`, `f8d2d62e`, `f8661739`) | ✅ (V1 caveat on key derivation) |
| Scope: Deletions | 🔲 | Code/config references gone. `packages/*/docs` and `docs/ft` references still owed at wrap (listed under Implementation status) | ⚠️ |
| Scope: Dependency approvals | 🔲 | Neither `hkdf` nor `zeroize` taken | ✅ N/A |
| Scope: Testing | 🔲 | Unit + acceptance exist. `tddy-daemon`/`tddy-session-lifecycle` suites not run locally | ⚠️ gaps T1–T6 |
| Scope: Package docs | 🔲 | `tddy-credentials/docs/credential-store.md` done. `auth-service.md` and others owed | ⚠️ |
| Scope: Code Quality | 🔲 | Score B (new code) → would tick. CI not yet read | ⚠️ pending CI |
| "2/9's `poll_device_login` must set `vault_unlock_key`" | owed by 2/9 | Already done in `auth_service.rs` `poll_device_login` | ✅ (note is stale) |

These scope ticks are recommendations. This run edited only this section.

### Acceptance criteria (PRD list)

| Criterion | Status |
|---|---|
| Unreadable from the file alone | ✅ (V6: not true once `U` leaks) |
| `label`/`metadata` inside the AEAD | ✅ |
| Header carries KDF name/version/params; change is `FormatMismatch` | ✅ (`kdf_version` branch untested) |
| A second login by the same user opens the same vault | ❌ **Only with the same token**. Real GitHub mints a new token on every exchange (V1) |
| Changed credential → `Locked`, no re-init, no second key | ✅ in the store. ❌ as UX: the login is refused with no remedy (V1) |
| Every successful login re-wraps | ✅ |
| Failed write fails the login; `Locked` fails the login | ✅ (the `Locked` acceptance test goes through the stub path only, T2) |
| No store API returns a secret to an RPC response path | ⚠️ By convention only: `CredentialRecord.secret` is a `pub String` with derived `Debug`/`Serialize` (V5) |
| Key material zeroized on drop, not cached beyond the session | ❌ Cached until process exit (V3). Secret strings and cipher/HMAC state are not wiped |
| Subject bound into the derivation | ✅ |
| A stub login seals nothing | ✅ |
| `GitHubTokenStore`/`FileGitHubTokenStore`/`github-tokens.json` gone | ✅ code/config. ⚠️ docs owed |
| PR-stack live status behaves identically | ✅ `pr_lookup_for_caller` unchanged (lifecycle suite not run locally) |
| Restart + refresh reopens the vault with no new login | ✅ (single tab; V4 for multi-tab) |
| Unlock key rotates on every refresh | ✅ (TOCTOU, V4) |
| Logout removes the lineage's slot | ✅ |
| Vault file never holds an unlock key; stub gets none | ✅ |

### Security review — verified correct

- **HKDF.** Matches RFC 5869: PRK = HMAC(salt, IKM), T(1) = HMAC(PRK, info‖0x01). An empty salt is equivalent to HashLen zero bytes. The test checks it against vector A.1.
- **Nonces.** A fresh 96-bit OsRng nonce per seal, which is ample at this volume.
- **Records.** Record AAD binds the keyed id, and the inner identity is re-hashed and compared (`ct_eq`).
- **Header.** The header is AAD of the login slot, so a salt edit gives `Locked` (tested).
- **`Locked` leaves the file alone.** `open_or_create`, `open_existing` and `open_with_unlock_key` never write on a failed open (tested).
- **Writes.** Every write goes through `write_atomic_with_mode(…, 0o600)`, with fsync and rename, under the process `WRITE_LOCK`. Slot eviction and rotation are single atomic rewrites.
- **Filenames.** The subject is hex-encoded into the filename, so there is no traversal and no case-fold collision.
- **Refresh subject check.** `reopen_the_vault` requires `unlock.subject() == login`.
- **Logging.** The unlock key is never logged. `UnlockKey`/`SecretBytes` `Debug` redact the key.
- **Responses.** No response carries the GitHub token.
- **Web client.** It clears the key on logout, on an `Unauthenticated` refresh and on a failed exchange, and keeps it on a transient failure.

### Findings

**Blocker**

- **V1** `tddy-github/src/auth_service.rs` `retain_the_login_credential` → `tddy-credentials/src/sessions.rs:61-74` → `vault.rs:171`.
  - **Problem.** The login KEK is derived from the GitHub access token, and a GitHub OAuth App issues a **new** token on every code or device exchange (up to 10 live per user, app and scope). "Does not expire" is not "stable".
  - **Failure scenario.** The daemon restarts, and then a user signs in fresh: a new browser, a lapsed 7-day refresh token, after a logout, with cleared storage, or with an evicted slot. The vault is `Locked` and `failed_precondition` refuses the login. The only remedy is deleting `credentials-<hex>.vault` by hand, and no in-product re-link exists before 4/9.
  - **Why the tests miss it.** Every fake returns a constant token.
  - **Options (developer's decision).** (a) Sign in anyway, report the vault locked, and add an explicit "reset vault". (b) Derive the KEK from something actually stable, such as a passphrase or a WebAuthn PRF. (c) Make the browser-held slots the only reopen path.
  - **Test to add.** A restart followed by a second real login with a different token.

**Should-fix**

- **V2** `sessions.rs:63-71`. A cached handle is reused even after its file was deleted or replaced. `rewrap` then fails with `Io("gone")` or `Locked`, and *every* login for that user fails until the daemon restarts. `github_pr_credentials.rs:97` meanwhile tells the user to "sign in again", which cannot help. Fix: on `Locked`/`Io` from a cached handle, drop the entry and reopen from disk.
- **V3** `sessions.rs:14`. `TODO(keyring)`, no issue reference. The data key stays in memory after every lineage has logged out, so PR status still performs with the token and the daemon reads credentials with nobody present. This fails the zeroize/cache criterion. Fix: evict on the last `forget`, or on a TTL tied to the refresh-token window.
- **V4** `sessions.rs:82-91` and `vault.rs:421`. The slot key is checked outside `WRITE_LOCK`, and `rotate_unlock_slot` takes only the slot id.
  - **Failure scenario.** Two tabs share `localStorage` and both refresh on load (`useAuth.ts:175`). Both pass the check and both rotate. `localStorage` can end holding the losing key, or `""` from a tab whose key was already rotated, which erases the good one. A lost response has the same effect.
  - **Fix.** Rotate by `&UnlockKey`, re-verified under the lock. Add cross-tab coordination (`navigator.locks` or BroadcastChannel), or give the previous key a short grace period.
- **V5** `record.rs:74-84`. `secret: String` is public, with derived `Debug`/`Serialize`, and is never zeroized. Boundary line 1 was meant to be a type property. Fix: a `SecretString` newtype with redacted `Debug`, no wire `Serialize`, wipe on drop and `expose()`.
- **V6** `packages/tddy-credentials/docs/credential-store.md` ("they already hold the disk"). The justification is wrong: disk plus `U` gives plaintext. `U` crosses plain-http at every refresh, and old backups keep old slots, which rotation cannot reach. Restate the trade-off honestly.
- **V7** `tddy-credentials/Cargo.toml:27`. The crate depends on `tddy-core` only for `write_atomic_with_mode`, which pulls tokio, jsonschema, ACP, workflow, git, task, rpc and sandbox into 6/9 and 7/9. This contradicts the stated crate-boundary choice, and `tddy-core/src/atomic_file.rs` itself says to name `tddy_session_store::atomic_file`. Depend on that crate or extract a leaf crate.

**Nits**

- **V8** `vault.rs:140`. The lock is process-local, so two processes on one `auth_storage` lose updates. The old store had the same limit. Add `flock`, or record it for 6/9.
- **V9** `vault.rs:115,125`. The login `info` for subject `unlock/<slot>/x` equals the unlock `info`. It is not exploitable (different salt and ikm) but is weak domain separation. Add a `login/` label.
- **V10** No rollback protection. Someone with write access can restore a removed or evicted slot or an older record. Record this as a stated limit.
- **V11** `sessions.rs:62`. Blocking fs work and fsync run under a `std::sync::Mutex` inside async handlers, and this lock blocks every user's PR-status `get`.
- **V12** `header.info` is written but never read (`derive_kek` uses `info_for(subject)`).
- **V13** `auth_service.rs` `github_record`. `updated_at` uses `.unwrap_or_default()`, a silent 0.
- **V14** `vault.rs:701`. The nonce length is a literal `12`.

### From /validate-tests

52 tests analyzed across 10 files. No `#[ignore]`, `.only` or `.skip`, no sleeps, and all use tempdirs. Tests are deterministic.

- **T1 (should-fix).** Every fake login returns the same token (`THE_GRANTED_TOKEN`, `THE_LOGIN_CREDENTIAL`), so the suite encodes V1's false premise and cannot catch it.
- **T2 (should-fix).** `tddy-daemon-auth/tests/login_opens_the_credential_store_acceptance.rs:33` `a_credential_store_sealed_under_another_key_refuses_the_login` runs `stub: true`, so it exercises `open_existing`. Its *Given* describes the real-login path (`unlock` → `open_or_create`), which has no acceptance test for `Locked`.
- **T3 (should-fix).** Security branches have no tests:
  - a refresh presenting another user's key (the subject filter);
  - a sealed record moved to another id (the `ct_eq` branch, `vault.rs:536`);
  - a refresh whose key does not open still succeeding with `""`;
  - `RefreshSessionResponse` and `PollDeviceLoginResponse` checked for a GitHub-token leak (only `ExchangeCode` is);
  - a rotate race.
- **T4 (nit).** `tddy-credentials/tests/credential_store_acceptance.rs:73` "altering…label" flips the last hex digit, which is in the Poly1305 tag, not the label. Rename it or target the label.
- **T5 (nit).** The `kdf_version` `FormatMismatch` branch is untested.
- **T6 (should-fix, web).** Untested:
  - logout sending the key and clearing it;
  - an `Unauthenticated` refresh clearing the key;
  - an empty returned key removing the stored one;
  - the page-load `refreshNow`.
- **T7 (nit, fluent-tests).** These have no Given/When/Then and bundle several behaviours into one tuple assert: `vault.rs:1042` `an_unlock_key_survives_its_wire_form…`, `kdf.rs` `hex_round_trips…` and `sessions.rs:202`. `past_the_bound…` asserts three facts.
- **T8 (nit).** `call`, `a_daemon_retaining_credentials_in` and `a_demo_daemon_retaining_credentials_in` are duplicated across two `tddy-daemon-auth` test files. `ProviderWithARealCredential` is duplicated between `tddy-github` and `tddy-daemon-auth`.

### From /validate-prod-ready

Status ⚠️ Gaps. No mock code in production, no debug output, and no `println!` in TUI paths.

- **TODO/FIXME.** Two markers without an issue reference: `sessions.rs:14` (AC-relevant, V3) and `vault.rs:669` (`zeroize`).
- **Fallbacks.** `unwrap_or_default` on `updated_at` (V13). The empty unlock key on a failed reopen is a recorded developer decision.
- **Unused.** `header.info` (V12). `list`, `remove` and `unlock_slot_ids` are published surface for 4/9, so acceptable.

### From /analyze-clean-code

**Score: B for new code.** The two functions over 60 lines, `build_auth_entries_with` (103) and `pr_status_for_caller` (75), are pre-existing and only touched.

- **Needs attention.** `github_pr_credentials.rs` `retained_github_token` is 44 lines; extract the "not open yet" branch.
- **Oversized: `vault.rs`, 1071 lines, new.**
  - Split: `format.rs` ← Header/VaultFile/read/write/`check_format`; `crypto.rs` ← seal/open/wrap/unwrap/KEKs/random; `unlock.rs` ← `UnlockKey` and the slot methods; `vault.rs` ← `CredentialStore`/`SessionVault`.
  - Cost: `pub(crate)` helpers only, and no consumers to repoint.
  - Recommend splitting now or before 4/9.
- **Oversized: `auth_service.rs`, 847 lines, grown by about 130.** Extract the vault glue into a submodule. Split later.
- **Magic values.** `vault.rs:701` `12`. The metadata keys `"github_id"`/`"avatar_url"` should be constants, since 4/9 will read them.
- **Duplication.**
  - The random → `SecretBytes::new` → `wipe` sequence repeats about 3 times; use `SecretBytes::random()`.
  - `random_16`/`random_32` are near-duplicates.
  - `ProviderId` and `AccountId` are identical newtypes.
  - The test helpers are duplicated (T8).

### Resolution (replan, 2026-09-23)

| Finding | Resolution |
|---|---|
| **V1** blocker | ✅ Passphrase KEK (Argon2id); the token is a record. Pinned by `after_a_restart_a_login_with_a_new_token_opens_the_vault_once_the_passphrase_is_given` |
| V2 stale handle | ✅ `SessionVault::is_stale` (file gone, or replaced so the data key is `Locked`); `SessionVaults::get` drops it, `retain` drops one replaced between lookup and write. Other I/O failures stay real failures |
| V3 cached until exit | ✅ for logout: the last slot's removal drops the handle. ⚠ lapsed-without-logout → `docs/dev/todo/2026-09-23-credential-vault-open-past-its-last-session.md` (the `sessions.rs` TODO points there) |
| V4 rotation race | ✅ `rotate_unlock_slot(&UnlockKey)` proves the key under the write lock; a 30 s grace answers the retired key with the same successor. Server grace chosen over `navigator.locks`, which needs a secure context |
| V5 secret type | ✅ `SecretString` (redacted `Debug`, wiped on drop, no serde, `expose()`); `CredentialRecord` sealed through a private mirror; the passphrase is one too |
| V6 trade-off prose | ✅ `credential-store.md` § *What the unlock key and the passphrase buy, and what they do not* |
| V7 `tddy-core` | ✅ Dropped. `tddy-session-store` is not light (tokio, jsonschema, workflow/task/actions), so the ~45-line writer is inlined (`atomic.rs`); duplication → `docs/dev/todo/2026-09-23-atomic-file-leaf-crate.md` |
| V8 process-local lock | ℹ Stated limit in `credential-store.md`; for 6/9 |
| V9 HKDF labels | ✅ `passphrase/`, `unlock/`, `record-id`, all `v2` |
| V10 rollback | ℹ Stated limit in `credential-store.md` |
| V11 blocking fs under a mutex | ℹ Unchanged; refreshes are now also serialised by the rotation lock |
| V12 `header.info` unused | ✅ Removed from the v2 header; the subject is bound by the KEK's `info` |
| V13 `unwrap_or_default` | ✅ An explicit `internal` error, logged |
| V14 literal `12` | ✅ `NONCE_BYTES` |
| T1 constant tokens | ✅ `GitHubMintingATokenPerExchange` (daemon-auth `tests/support`) |
| T2 stub-only `Locked` | ✅ `a_real_login_over_a_vault_this_daemon_has_not_opened_signs_in_and_leaves_the_file_alone` |
| T3 security branches | ✅ another user's key, record moved to another id, key that no longer opens, refresh/device-login leak checks, the race. The id-swap is caught by the AEAD's associated data before the inner `ct_eq`; the `ct_eq` stays as defence in depth |
| T4 label test name | ✅ Renamed `altering_a_sealed_record_is_detected_rather_than_absorbed`, its comment says the Poly1305 tag |
| T5 `kdf_version` | ✅ `a_header_from_another_argon2_version_is_reported_as_a_format_mismatch` |
| T6 web | ✅ Logout sends then clears, `Unauthenticated` clears, empty key removes, page-load refresh — `sessionTokenStore.test.ts`; logout and the page-load refresh moved into the store to be testable |
| T7 bundled asserts | ✅ in `vault.rs` (wire form, bound); ℹ `kdf.rs` `hex_round_trips…` left as is |
| T8 duplicated helpers | ✅ within `tddy-daemon-auth`; ℹ `ProviderWithARealCredential` across crates stays (no shared testkit) |

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Publish the draft-PR contract — wave 2
- [x] M1–M12
- [x] Ask before taking `hkdf` / `zeroize` — neither taken
- [ ] Package documentation for the five affected packages
- [ ] `/wrap-context-docs` — this node claims **no** backlog entry and **no** code-issue record

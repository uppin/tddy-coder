# Changeset: Session-gated encrypted credential store

**Date**: 2026-09-19
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Stack**: `#keyring` 3/9 · branch `feature/keyring/store` · PR [#510](https://github.com/uppin/tddy-coder/pull/510) · base `master` (parents #508 and #509 are merged)

## Affected Packages

- **tddy-credentials** (**new crate**): `docs/credential-store.md` — the record model, the sealed
  file format, the passphrase key derivation, the registry of open vaults and its states
  - `src/vault.rs` split into `vault/{format,crypto,unlock}.rs` (replan step A, behaviour-preserving)
  - `src/atomic.rs` — the owner-only swap-then-rename writer, inlined so the crate no longer depends
    on `tddy-core` (V7; duplication recorded in `docs/dev/todo/2026-09-23-atomic-file-leaf-crate.md`)
- **tddy-github**: `src/token_store.rs` — **deleted**; `src/auth_service.rs` writes through the new
  store. At wrap, its vault half moved into `src/auth_service/vault.rs` (see `## Restructuring`),
  with the unlock throttle in `src/auth_service/vault/backoff.rs`
- **tddy-daemon-auth**: [auth-service.md](../../../packages/tddy-daemon-auth/docs/auth-service.md)
  - `src/github_token_store.rs` — **deleted**
  - `src/auth.rs:168-275` — vault construction; the half-login rule extended to "cannot open"
- **tddy-daemon**: [daemon-endpoint.md](../../../packages/tddy-daemon/docs/daemon-endpoint.md)
  - `src/runtime.rs:1076` — construction and injection. **The file is not split**
- **tddy-session-lifecycle**: [session-service.md](../../../packages/tddy-session-lifecycle/docs/session-service.md)
  - `src/connection_service/handler_state.rs:68` — the `credential_vaults()` accessor and field
- **tddy-daemon-rpc** (added at the rebase onto `master`): [architecture.md](../../../packages/tddy-daemon-rpc/docs/architecture.md)
  - `Cargo.toml` — a path dependency on `tddy-credentials` (internal; no external crate)
  - `src/pr_stack.rs` — `PrStackRpcHandler.credential_vaults`, supplied by
    `DaemonSessionHost::credential_vaults()`
  - `src/pr_stack/pr_status.rs:56` — the one external read, migrated (`retained_github_token`)
- **tddy-service** (added at green): `proto/auth.proto` — additive `vault_unlock_key` fields on
  `ExchangeCodeResponse`, `PollDeviceLoginResponse`, `RefreshSessionRequest`,
  `RefreshSessionResponse` and `LogoutRequest`; at the replan, a `VaultState` enum on
  `ExchangeCodeResponse`, `PollDeviceLoginResponse`, `RefreshSessionResponse` and
  `GetAuthStatusResponse`, and **two new RPCs**, `UnlockVault` and `ResetVault`. At wrap: `build.rs`
  generates `UnlockVaultRequest` / `ResetVaultRequest` without prost's `Debug`, and
  `src/auth_redacted_debug.rs` prints them redacted (N1)
- **tddy-web** (added at green): `src/rpc/sessionTokenStore.ts`, `src/hooks/useAuth.ts` — the unlock
  key is stored beside the refresh token, presented on refresh, replaced with the rotated one, sent
  on logout, cleared with the tokens; a page load holding one refreshes at once. At the replan:
  `src/components/CredentialVaultPrompt.tsx` (mounted in `src/index.tsx`) — the passphrase prompt
  for a `LOCKED` or `UNINITIALIZED` vault, with a forgot-passphrase reset. At wrap:
  `src/lib/vaultPassphrase.ts` — the passphrase length rule, counted in code points (N4)
- Incidental, one line each: `tddy-remote-git-repo` and `tddy-session-sync` (a tool's
  `RefreshSessionRequest` presents no unlock key), `tddy-host-service` (a doc link to the deleted
  store), `tddy-daemon-kernel` (the `auth_storage` doc comment), `tddy-rust-typescript-tests`
  (regenerated `auth_pb.ts`), `daemon.yaml.production`, `desktop.yaml.production`, `install`
- Backlog entries added at the replan: `docs/dev/todo/2026-09-23-atomic-file-leaf-crate.md`,
  `docs/dev/todo/2026-09-23-credential-vault-open-past-its-last-session.md` (narrowed at wrap, S5)
- Added at wrap: `docs/dev/todo/2026-09-24-credential-vault-cipher-key-schedule-not-wiped.md`,
  `docs/dev/todo/2026-09-24-keyring-store-deferred-auth-and-attach-splits.md`, and the code-issue
  records `packages/tddy-daemon-auth/docs/code-issues/oversized-file-auth.md` and
  `packages/tddy-session-sync/docs/code-issues/oversized-file-attach.md`

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

Its **only reader outside the auth crate** is the PR-status read, now `tddy-daemon-rpc/src/pr_stack/pr_status.rs:56` (it
was `svc_pr_status_for_caller.rs:93` before `#carve` moved it; `PrStackRpcHandler` gets the vaults
from `DaemonSessionHost::credential_vaults()`, `handler_state.rs:68`); the rest is construction and
plumbing (`auth.rs:168-275`, `runtime.rs:1076`).

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

**Parent in the line**: `#keyring` 2/9 `desktop-login` — [#509](https://github.com/uppin/tddy-coder/pull/509),
**merged** (squash, `35cf2913`); this PR's base is now `master`.

**Real dependency edge**: `#keyring` 1/9 `signing-key` — [#508](https://github.com/uppin/tddy-coder/pull/508),
**merged**.
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
> *where* `stored` comes from changes (now `tddy-daemon-rpc/src/pr_stack/pr_status.rs:56`). A second copy of those
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

`runtime.rs:623` (`build`), covering the `:1076` token-store wiring this node rewires. Recorded,
**not claimed**: two independent backlog entries and this repo's planning policy say a mechanical
split inside a feature PR buries the reviewable diff. Replacing the store's construction makes the
function marginally smaller as a side effect, which is the only size change this node makes.

### ⚠ DURING — PR-stack status polling and stack hygiene — [`2026-07-26-pr-stack-status-polling-and-stack-hygiene.md`](../todo/2026-07-26-pr-stack-status-polling-and-stack-hygiene.md)

The PR-status read (`pr_stack/pr_status.rs`) is the one external reader this node migrates, and the entry is about
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
claimed** — this node touches the PR-status read, not the sandbox or the action runner,
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
- [x] **Migration**: `tddy-daemon-rpc/src/pr_stack/pr_status.rs:56`, via `handler_state.rs` `credential_vaults()`
- [x] **Dependency approvals**: neither `hkdf` nor `zeroize` taken
- [x] **Testing**: unit + acceptance, scoped (see *Measured state (replan)* and *Measured state
      (wrap)*)
- [ ] **Package Documentation**: ✅ `tddy-credentials/docs/credential-store.md`; ⚠ the other
      packages' docs are owed at wrap
- [ ] **Code Quality**: ✅ scoped clippy clean on every touched Rust package, and the file-length
      gate met for `auth_service.rs` (621 → 379, `## Restructuring`); `auth.rs` and `attach.rs`
      deferred with consent (`docs/dev/todo/2026-09-24-keyring-store-deferred-auth-and-attach-splits.md`);
      ⚠ CI on the wrap commits not yet read

## Technical Changes

### State A (Current)

- `GitHubTokenStore` — `put(login, access_token)` / `get(login)`; one implementation over a `0600`
  plaintext JSON `HashMap<String, String>`, serialised by a process-wide `PUT_LOCK`.
- One external reader: the PR-status read, now `tddy-daemon-rpc/src/pr_stack/pr_status.rs:56`.
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
- **Implementation**: `runtime.rs:1076` constructs and injects the store. **Not split.**

#### tddy-session-lifecycle
- **Integration**: `DaemonSessionHost::credential_vaults()` (`handler_state.rs:68`) hands the registry
  to the PR-stack handler.

#### tddy-daemon-rpc
- **Integration**: `tddy-daemon-rpc/src/pr_stack/pr_status.rs:56` reads through `retained_github_token`.
  `pr_lookup_for_caller`'s three outcomes are unchanged.

## Implementation Milestones

- [x] **M1** — `tddy-credentials`: record model, versioned header, seal/open, `write_atomic_with_mode`
- [x] **M2** — key derivation, wrapped data key, verifier, `rewrap`, zeroization — plus unlock slots
      (the login-derived KEK and `rewrap` superseded by M11)
- [x] **M3** — wire construction in `auth.rs` and `runtime.rs`; derive on login, re-wrap on login
      (superseded by M11: a login retains its token by vault state);
      unlock key on login, reopen + rotate on refresh, remove on logout
- [x] **M4** — migrate the PR-status read, now `tddy-daemon-rpc/src/pr_stack/pr_status.rs:56` (through `retained_github_token`)
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

**Wrap (2026-09-24)** — the `/pr-wrap` findings S1–S7 and N1–N10:

- [x] **M13** — split `tddy-github/src/auth_service.rs` (see `## Restructuring`), `20cc938a`
- [x] **M14** — S1–S6, N2, N3, N7, N8 in `tddy-credentials` / `tddy-github` / tests, `d3f86e28`
- [x] **M15** — N1 in `tddy-service`, `63a1e058`
- [x] **M16** — S3 (client), N4, S7 in `tddy-web`, `beaae788`
- [x] **M17** — N5, N6, N9, N10 documented; the deferred splits and the zeroize backlog entry

**Implementation status (green).** All milestones done, including the replan's; see
*Measured state (replan)* for the scoped counts. Still owed at wrap, because `packages/*/docs/` moves only through this changeset:
`tddy-daemon-auth/docs/auth-service.md` (lines 18, 236, 260 name `github_token_store`),
`tddy-github/docs/device-flow.md:202`, `tddy-session-lifecycle/docs/session-service.md:60,100`,
`tddy-daemon-rpc/docs/architecture.md:52,94`,
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

### Measured state (wrap)

| Run | Command | Result |
|---|---|---|
| Split baseline and after | `./test -p tddy-github` | 92 passed / 0 failed, both (doc-tests: 0) |
| Wrap fixes | `./test -p tddy-credentials -p tddy-daemon-auth -p tddy-github --no-fail-fast` | **294 passed / 0 failed** |
| N1 | `cargo test -p tddy-service --test auth_passphrase_redaction --test unbundle_service_split` | 2 / 0 and 28 / 0 |
| Web | `bun test src/hooks src/lib src/rpc` | 594 / 0 |
| Web | `cypress run --component --spec cypress/component/CredentialVaultPromptAcceptance.cy.tsx` | 12 / 0 |

The final scoped numbers, after the documentation commit, are in the `/pr-wrap` report of this run.

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
      logout, and a logout with no unlock key now drops the token its login left pending (S5,
      `credential_vault_guard_acceptance.rs`); ⚠ a lineage that lapses without a logout keeps it
      until exit (`docs/dev/todo/2026-09-23-credential-vault-open-past-its-last-session.md`), and the
      cipher key schedules are not wiped (`docs/dev/todo/2026-09-24-credential-vault-cipher-key-schedule-not-wiped.md`)
- [x] Creating or resetting a vault needs a fresh sign-in; an access token alone cannot, a reset
      is refused while the vault is open, and set-aside vaults are capped with nothing deleted (S1 —
      `credential_vault_guard_acceptance.rs`, `sessions.rs` tests)
- [x] Wrong passphrases are throttled per user, and derivation never runs on an RPC worker (S2 —
      `past_the_free_wrong_passphrases_the_next_attempt_is_told_how_long_to_wait`, `backoff.rs` tests)
- [x] A refresh that cannot read the vault keeps the browser's key (S3 —
      `a_refresh_that_cannot_read_the_vault_hands_back_the_presented_key_unrotated`,
      `sessionTokenStore.test.ts`)
- [x] A stub login creates nothing and reports `NONE`
- [ ] `GitHubTokenStore`, `FileGitHubTokenStore` and all `github-tokens.json` references are gone —
      ✅ code and config; ⚠ docs owed at wrap
- [x] PR-stack live status behaves identically, and says to unlock the credential vault while it is
      closed

## Restructuring

**Developer-approved decomposition at wrap**: `packages/tddy-github/src/auth_service.rs` crossed the
500-production-line gate (622 production lines counted to the first `#[cfg(test)]`; about 260 added
by this PR). Its credential-vault half moved into `src/auth_service/vault.rs`, in its own
behaviour-preserving commit `20cc938a` (`refactor(github): …`).

| File | Production lines before → after | Total lines after |
|---|---|---|
| `tddy-github/src/auth_service.rs` | **621 → 379** | 1,050 (tests unchanged) |
| `tddy-github/src/auth_service/vault.rs` (new) | — → 290 | 290 (no tests) |

- **Moved**: `retaining_vaults`, `vault_state_of`, `retain_the_login_credential`,
  `reopen_the_vault`, `caller_login`, `vaults_to_unlock`, the `UnlockVault` / `ResetVault` bodies
  (`unlock_the_vault`, `reset_the_vault`), the logout's slot removal (`forget_the_lineage`), and the
  free helpers `check_new_passphrase`, `to_proto_state`, `github_record`, `refused_by_the_vault`,
  `GITHUB_ID_METADATA`, `AVATAR_URL_METADATA`. The trait methods delegate.
- **Public API unchanged**: `auth_service` re-exports the two metadata constants; private helpers
  became `pub(super)`; no consumer was repointed.
- **By hand, not by the restructure engine.** Every moved method is an impl member, which
  `restructure anchors` cannot name, and cutting the impl is the E3 seam the engine refuses (memory:
  restructure engine defects, 2026-09-23). The split moved whole line ranges with a script and no
  hand-written code; `cargo check --all-targets` and clippy were clean.
- **Green baseline**: `./test -p tddy-github` 92 passed / 0 failed before and after (doc-tests 0).
- The wrap's behaviour fixes then grew `vault.rs` (S2, S3, S5, N3), not `auth_service.rs`: after
  them `auth_service.rs` is 384 production lines and `vault.rs` 433 (no tests), with the throttle's
  backoff in `vault/backoff.rs` (75 production lines, plus its unit tests).

## Validation Results

**Last run**: 2026-09-24, `/pr-wrap` analysis steps (validate-changes → validate-tests →
validate-prod-ready → analyze-clean-code) plus a security review of the passphrase design. Diff
range `origin/master..HEAD`: 10 commits, 62 files, tip `62bd0800`. It replaces the 2026-09-23
section, which reviewed the superseded token-derived design. Its V1–V14 and T1–T8 are resolved, as
recorded in that run's Resolution table, and are not repeated here.

**Overall: ⚠️ no blocker. Seven should-fix findings, S1–S7; S1–S3 are security. CI is still
running.**

### Stack gate

| Check | Result |
|---|---|
| Stack branch | Yes, planned (`feature/keyring/store`, PR #510). Parents #508 and #509 are squash-merged. The PR base is now `master` |
| Rebase | ✅ Already current: merge-base = `origin/master` = `35cf2913` |
| Leak check | ✅ Clean: `origin/master..HEAD` holds only this PR's 10 commits |
| Diff contains only this PR's files | ✅ Every file maps to an Affected Packages entry, except `tddy-daemon-rpc` (see *Stale text*) |
| Parent-owned files intact | ✅ Only `token_store.rs` and `github_token_store.rs` are deleted, both planned (M5) |
| `## Dependencies` not implemented here | ✅ No 1/9 or 2/9 symbol is re-implemented |
| `## Boundaries` respected | ✅ No Accounts RPC, `screen_sharing_vault.rs` untouched, `runtime.rs` not split, neither `hkdf` nor `zeroize` taken. The only new crate edges are the internal `tddy-credentials` ones |
| `## Responsibility` delivered | ✅ In full. No `todo!()` or `unimplemented!()` remains in `src` |

### What the rebase changed (verified)

- **M4 moved.** The PR-status read is now `packages/tddy-daemon-rpc/src/pr_stack/pr_status.rs:56`,
  which calls `retained_github_token(self.credential_vaults…)`. `PrStackRpcHandler.credential_vaults`
  is supplied by `DaemonSessionHost::credential_vaults()`
  (`tddy-session-lifecycle/src/connection_service/handler_state.rs:68`). The same fact holds for
  both `svc_pr_status_for_caller.rs`, which still exists, and the old reader, which no longer reads
  a token.
- **`tddy-daemon-rpc`** gains a path dependency on `tddy-credentials`. It is internal; no external
  crate is added (`Cargo.lock` gains only the `tddy-credentials` package).
- **`useAuth.ts`.** `vaultUnlockKey` and `vaultState` flow through `WholeSession`,
  `checkWholeSession` and `signedInState`. An empty unlock key is deliberately not a missing part.
- **Tests** use `Request::direct` and `RequestMetadata::over(RequestTransport::Direct)`, and both
  test providers implement the `Result`-returning `authorize_url`
  (`tddy-daemon-auth/tests/support/mod.rs:263`, `tddy-github/tests/github_token_retention_acceptance.rs:79`).
- **`complete_login`** runs admission, then the signing check, then `retain_the_login_credential`
  (`auth_service.rs:357-375`). A refused login therefore leaves no pending token and no slot.

### Stale text (fix before wrap)

- **This changeset.**
  - Line 6 names the base `feature/keyring/desktop-login` (#509); the base is now `master`.
  - `## Dependencies` still presents #508 and #509 as open parents, but both are merged.
  - Every `svc_pr_status_for_caller.rs:93` reference is stale (Affected Packages, Background,
    Draft PR contract, Scope, State A, Delta, M4). The read is at
    `tddy-daemon-rpc/src/pr_stack/pr_status.rs:56`.
  - `runtime.rs:882` is now `:1076`.
  - `auth.rs:83-152` is now `:168-275`.
- **Affected Packages** is missing `tddy-daemon-rpc`: `Cargo.toml`, `pr_stack.rs` and
  `pr_stack/pr_status.rs`. Change `tddy-session-lifecycle`'s entry to "the
  `credential_vaults()` accessor and field".
- **The PRD.**
  - Its header says "depends on 1/9"; 1/9 is merged.
  - It cites `svc_pr_status_for_caller.rs:93` at lines 50, 176, 210 and 253.
  - The Technical Impact table has no `tddy-daemon-rpc` row.
- **Docs owed at wrap.** The Implementation status list is incomplete, and its line numbers moved.
  - `tddy-daemon-auth/docs/auth-service.md`: now lines 18, 236 and 260.
  - Also owed, and not on the list: `tddy-github/docs/device-flow.md:202`,
    `tddy-session-lifecycle/docs/session-service.md:60,100` and
    `tddy-daemon-rpc/docs/architecture.md:52,94`.
- **`daemon.yaml.production`** still says "openable only from their login", which is the
  superseded design.

### Build (scoped; as measured before this run, not re-run)

| Package(s) | Result |
|---|---|
| tddy-credentials, tddy-daemon-auth, tddy-github | ✅ `./test -p …`: 267 passed, 0 failed |
| tddy-daemon-rpc | ✅ `query_branch_resolution_acceptance`, `orchestrator_repo_root_resolution_acceptance`, `rpc_handlers_shape` and `rpc_handlers_acceptance`: 34 passed |
| tddy-service | ✅ `unbundle_service_split` and `service_coordinates`: 29 passed |
| All 11 touched Rust packages | ✅ `cargo check --all-targets` clean |
| tddy-daemon-rpc, tddy-session-lifecycle, tddy-daemon-auth, tddy-github | ✅ `clippy -D warnings` clean |
| tddy-web | ✅ `bun test src/hooks src/lib src/rpc`: 589 passed. Cypress CredentialVaultPrompt, DeviceLogin, AuthProviderRefresh and DurableSession: 48 of 48 |
| Generated code | ✅ In sync |
| **CI (#510, head `62bd0800`)** | ⏳ Build, arm64 build, Generated code and both VM boots pass. Rust tests, Web tests, Rust lint and Cloudinit+nix are still pending |

Nothing workspace-wide was run locally. Whole-workspace health is CI's.

### Acceptance criteria (PRD list)

| Criterion | Status |
|---|---|
| Unreadable from the file alone; no passphrase on disk | ✅ |
| `label`/`metadata` inside the AEAD | ✅ |
| Header names the KDF and its parameters; a change is `FormatMismatch` | ✅ Checked by name before derivation, and the header is the passphrase slot's AAD |
| After a restart, a fresh login with a new token → `LOCKED`, and the passphrase opens the same vault | ✅ |
| The first real login creates the vault under a chosen passphrase | ✅ |
| Wrong passphrase → `failed_precondition`, file unchanged, still signed in | ✅ |
| A login while open needs no passphrase and gets a slot | ✅ ⚠ except under the S4 race |
| Reset renames aside and seals the token into a fresh vault | ✅ |
| A failed write fails the login; a closed vault is reported | ✅ ⚠ S4 can report `OPEN` while the token sits pending |
| No secret on an RPC response path; no passphrase in logs, file or responses | ✅ (nit N1: prost `Debug` of the request) |
| Zeroized on drop, not cached beyond the session | ⚠ Unmet, as recorded: a lapsed lineage keeps the vault open. **Also** a pending token survives logout (S5) |
| Subject bound into the derivation | ✅ |
| Stub login seals nothing and reports `NONE` | ✅ |
| `GitHubTokenStore`/`github-tokens.json` gone | ⚠ Code and config are clean. The docs are owed, and the owed list is incomplete (see *Stale text*) |
| PR status behaves identically and names the unlock remedy | ✅ |
| Restart + refresh with the unlock key reopens the vault, with no passphrase | ✅ |
| The key rotates on every refresh, and two tabs keep a working one | ✅ |
| Logout removes the lineage's slot | ✅ |
| The file never holds an unlock key, and a stub is handed none | ✅ |

### Scope boxes

The recommended state for each box is below. Per this run's instruction, only this section was
edited.

- ✅ PRD, Changeset, Draft PR contract, New crate, Key derivation, Vault states and RPCs,
  Dependency approvals, Testing (scoped).
- ✅ Migration. The box text names a stale location.
- ⚠ Deletions and Package Documentation: docs owed at wrap.
- ⚠ Code Quality. The score is B, which would tick the box, but CI is still pending.

### Security review: verified correct

- **Argon2id.** The KDF is Argon2id v0x13 (m=19456 KiB, t=2, p=1) with a fresh 16-byte OsRng salt
  per create and per reset. `check_format` compares every cost by name before anything is derived.
  The whole `Header` is the passphrase slot's AAD, so an edited salt gives `Locked` (tested).
- **Domain separation.** Every HKDF `info` carries a label: `passphrase/`, `unlock/`,
  `record-id`. The unlock-slot AAD binds the format version, the KDF, the slot id and the subject.
- **Minimum length.** It is enforced server-side for both create and reset
  (`check_new_passphrase`, `auth_service.rs`, counted in chars). Unlock does not check it, which is
  correct.
- **Where the passphrase reaches.** `SecretString` has a redacted `Debug`, no serde, and is wiped
  on drop. No log line formats a request. A test proves that no passphrase appears in any log
  line, file or response (`login_opens_the_credential_store_acceptance.rs:347`).
- **No cross-user access.**
  - `UnlockVault` and `ResetVault` take the subject from a verified **access** token
    (`caller_login` rejects refresh tokens).
  - A refresh requires `unlock.subject() == login`.
  - A logout needs a key that opens its slot.
- **Grace window.** A replayed retired key gets the **same** successor. `reopen` never adds a
  slot, so no second lineage can be minted. Logout drops the rotation entry, and a reset drops
  every rotation entry for the subject.
- **Concurrent refreshes.** Refreshes serialise on `rotations`, and the key is re-proven under
  `WRITE_LOCK` in `rotate_unlock_slot`.
- **Unlock racing reset.** The unlock's `put` fails the verifier (`Locked`) and its pending
  records are restored.
- **`atomic.rs`.** The swap file is `create_new` at `0600`, named
  `.<name>.<pid>.<8 random bytes>.swap` in the same directory. `sync_all` runs before the rename.
  `auth_storage` is created at `0700` (`auth.rs` `ensure_owner_only_dir`).
- **Set-aside name.** `credentials-<hex>.locked-<unix>[-n].vault`. It cannot collide with any
  subject's `credentials-<hex>.vault`, because a subject's hex has no `.`.
- **Web.**
  - The passphrase lives only in React state and is cleared whenever `vaultState` changes. It is
    never written to `localStorage`.
  - The reset path shows a warning and needs the passphrase typed twice.
  - "Not now" only hides the modal, so the app stays usable.

### Resolution (wrap, 2026-09-24)

| Finding | Outcome | Where |
|---|---|---|
| S1 | ✅ Fixed. `create` / `reset` need a pending record from a fresh login (`NoFreshLogin`); reset refused while open (`AlreadyOpen`); at most `MAX_SET_ASIDE_VAULTS` = 5 set aside, **refused rather than deleted** past it (`TooManySetAside`). ⚠ Residual, documented: the gate is the pending record, not its age | `d3f86e28` |
| S2 | ✅ Fixed. Derivation on `spawn_blocking` behind a semaphore of 2; per-user backoff (3 free, 2 s doubling to 15 min), `RESOURCE_EXHAUSTED` naming the retry-after, checked once a turn is held. The semaphore itself has no test | `d3f86e28` |
| S3 | ✅ Fixed, server and client | `d3f86e28`, `beaae788` |
| S4 | ✅ Fixed: one `transitions` lock. The `Barrier` race test pins the invariant; it cannot reliably reproduce the microsecond window, and passed before the fix too | `d3f86e28` |
| S5 | ✅ Fixed: `discard_pending` at a key-less logout; backlog entry narrowed | `d3f86e28` |
| S6 | ✅ Tests added (the grace-window replay after logout already held; now pinned at unit and RPC level) | `d3f86e28` |
| S7 | ✅ Tests added; both passed at once, pinning behaviour that already held | `beaae788` |
| N1 | ✅ Fixed with prost-build's `skip_debug` plus a redacting `Debug` (the access token is redacted too) | `63a1e058` |
| N2 | ✅ Fixed: the directory fsync error propagates on unix | `d3f86e28` |
| N3 | ✅ Fixed: `MAX_PASSPHRASE_CHARS` = 1024, checked for unlock, create and reset | `d3f86e28` |
| N4 | ✅ Fixed: code points, in `src/lib/vaultPassphrase.ts`. The constants are still mirrored by hand, with a comment naming the Rust source | `beaae788` |
| N5, N6, N10 | Documented limitations, `credential-store.md` § Known limitations | docs commit |
| N7 | ✅ Used by the zero-grace test | `d3f86e28` |
| N8 | ✅ Grace compare is strict (`<`) | `d3f86e28` |
| N9 | ✅ Documented in the trade-off section | docs commit |
| `vault/crypto.rs` TODO | Backlog entry `2026-09-24-credential-vault-cipher-key-schedule-not-wiped.md`, referenced from the TODO | docs commit |
| Stale text | ✅ Changeset and PRD updated; `daemon.yaml.production` reworded (`desktop.yaml.production` had no such wording). Package docs stay owed at wrap | docs commit |

### Findings

**Blocker**: none.

**Should-fix**

- **S1 (security): `ResetVault` needs nothing but an access token.**
  - **Where.** `tddy-github/src/auth_service.rs` `reset_vault`, and `UnlockVault{create}` over an
    uninitialized vault.
  - **Scenario.** An attacker sniffs a 5-minute access token (or the 7-day refresh token, which
    sits in `localStorage` and crosses plain http). They call `ResetVault`. The victim's vault is
    set aside, and a fresh one is created under the attacker's passphrase. Any pending victim
    token is sealed into it. The victim's unlock keys stop working, and the victim's own
    passphrase now fails, so they must reset again.
  - **Impact.** A transient token becomes durable passphrase control, plus a DoS. It is
    repeatable, and every call leaves another aside file. The attacker **cannot** seal their own
    GitHub token into the vault, because pending records are keyed by the GitHub login that
    authenticated.
  - **Fix.** Allow a reset only right after a real GitHub login: require `holds_pending(login)`
    or a one-time proof from the exchange response, rather than the access token alone. Refuse it
    while the vault is `Open`. Cap the aside files.
- **S2 (security): no throttling on `UnlockVault`, and Argon2 blocks the async executor.**
  - **Where.** `auth_service.rs` `unlock_vault` → `SessionVaults::unlock` →
    `vault/crypto.rs:108` `hash_password_into`.
  - **Scenario.** Any admitted user creates their own vault, then floods `UnlockVault` with wrong
    passphrases. Each attempt costs about 19 MiB and tens of ms on a tokio worker thread, so the
    daemon's RPC executor starves for every user. For a victim's vault, a stolen access token
    permits unlimited online guessing against an 8-character minimum.
  - **Fix.** Run the derivation under `spawn_blocking` behind a small semaphore. Add per-subject
    exponential backoff on `Locked`.
- **S3 (security/UX): a refresh throws away the client's key on non-`Locked` errors.**
  - **Where.** `auth_service.rs` `reopen_the_vault` answers every error with `""`, including
    `Io`.
  - **Scenario.** A transient read failure (EIO, EMFILE) during a refresh makes the web client
    delete its only unlock key (`sessionTokenStore.ts` `storage.set(…, "")`). The lineage loses
    its slot for good, and the slot lingers on disk until eviction.
  - **Fix.** Return `""` only for `Locked`. On any other error, hand back the presented key
    unrotated.
- **S4 (race): a login racing an unlock strands its token.**
  - **Where.** `tddy-credentials/src/sessions.rs:155-177` `retain`.
  - **Scenario.** `get()` sees the vault closed. A concurrent `unlock` then runs `admit`, which
    seals the (empty) pending set and registers the handle. `retain` then calls
    `hold_pending(record)`, and returns `state()` = `Open` with `unlock_key: None`.
  - **Impact.** The client is told `OPEN`, holds no key and is never prompted. The new token sits
    in memory, unsealed, until some later unlock, create or reset. The same gap exists between
    `seal_pending` and `register` in `admit`.
  - **Fix.** Take one lock across the lookup and `hold_pending` that `admit` also holds while
    sealing and registering. Or, after `hold_pending`, re-check `get()` and seal at once.
- **S5: a pending GitHub token outlives logout.**
  - **Where.** `sessions.rs` `pending` and `auth_service.rs` `logout`.
  - **Scenario.** A lineage that signed in to a `LOCKED` or `UNINITIALIZED` vault holds no unlock
    key. Its logout therefore calls no `forget`, and the live `repo`-scoped token stays in daemon
    memory until the next restart or unlock. This is the same class as the recorded "lapsed
    lineage" gap, but reachable by an explicit logout.
  - **Fix.** Logout already verifies the access token; have it drop that subject's pending records
    too. Add the case to `docs/dev/todo/2026-09-23-credential-vault-open-past-its-last-session.md`.
- **S6 (tests): security edges not pinned.**
  - `retain` racing `unlock` (S4).
  - A pending token after logout (S5).
  - `ResetVault` with a too-short passphrase (only create is tested).
  - `UnlockVault`/`ResetVault` presented a **refresh** token (only a garbage token is tested).
  - A grace-window replay after logout.
- **S7 (web tests): the "Not now" path is untested.**
  - **Where.** `CredentialVaultPromptAcceptance.cy.tsx`.
  - **Missing.** A test that "Not now" hides the prompt and leaves the page usable, and that it
    comes back on a state change. A test that the passphrase never reaches `localStorage` is also
    absent (nit).

**Nits**

- **N1.** Prost derives `Debug` for `UnlockVaultRequest` and `ResetVaultRequest`, including the
  passphrase. Nothing logs them today. Add `skip_debug` or a redacting `type_attribute` in
  `tddy-service`'s build.
- **N2.** `atomic.rs:71-73` swallows the directory-fsync error, so "`put` returns `Ok` only once
  durably on disk" is not strictly true after a crash. Propagate the error, or log it and soften
  the doc.
- **N3.** No upper bound on passphrase length. Only the RPC frame limit applies.
- **N4.** The web prompt counts `passphrase.length` (UTF-16 units), but the server counts chars.
  A passphrase of astral characters passes the UI and is refused by the server. The constant
  `MIN_PASSPHRASE_CHARS` is also duplicated by hand in `CredentialVaultPrompt.tsx:8`.
- **N5.** `set_aside` checks `exists()` and then renames, and a `rename` overwrites on unix. That
  is a cross-process TOCTOU; within one process the `WRITE_LOCK` covers it.
- **N6.** `reopen` holds the daemon-wide `rotations` std mutex across file I/O and fsync inside an
  async handler, so every user's refreshes serialise (V11, still).
- **N7.** `SessionVaults::with_rotation_grace` has no caller; the test sets the field directly.
  Use it or drop it.
- **N8.** The `ROTATION_GRACE` test with `Duration::ZERO` relies on `elapsed() > 0`. Compare with
  `<`, not `<=`, to be robust on coarse clocks.
- **N9.** A set-aside vault keeps its old unlock slots, so an old `U` plus the aside file is still
  plaintext. `credential-store.md` mentions only the old passphrase.
- **N10.** Two tabs that both unlock by passphrase each add a slot, and the second overwrites the
  first's key in `localStorage`. The orphan slot lives until eviction.

### From /validate-tests

- **Coverage.** 11 test files, about 120 tests: `tddy-credentials` unit + acceptance, `tddy-github`
  retention, `tddy-daemon-auth` login/restart/support, `sessionTokenStore.test.ts` and the Cypress
  prompt spec.
- **Clean.** No `#[ignore]`, `.only` or `.skip`, and no sleeps. Temp dirs are used throughout.
  Races are driven by `Barrier`, with order-independent asserts.
- **Fluent-tests.**
  - Given/When/Then comments throughout.
  - The Cypress spec uses a page object, `mountWithRpc` and `anInMemoryRpcBackend`, with no raw
    selectors in bodies and no `cy.intercept`.
  - GitHub is faked by `GitHubMintingATokenPerExchange`.
- **Gaps.** S6 and S7 above.
- **Nit, style.** Many tests assert a 3–4 field tuple (for example
  `sessions.rs` `a_first_login_finds_no_vault_and_writes_nothing`). Each is one behaviour seen
  from several sides, which is acceptable, but a failure message is harder to read.
  `kdf.rs` `hex_round_trips…` is still bundled.
- **Nit, duplication.** `decoded_parts` is duplicated between
  `tddy-github/tests/github_token_retention_acceptance.rs:139` and
  `tddy-daemon-auth/tests/vault_unlock_across_restart_acceptance.rs:335`.
- **Timing.** The captured-log test installs a process-global logger. That is safe for a
  "nothing leaked" assertion, because extra lines only strengthen it.

### From /validate-prod-ready

Status ⚠️ Gaps (no blockers).

- **Mocks and debug output.** No mock or fake code in production paths. No `dbg!`, `println!` or
  `console.log`.
- **TODO/FIXME markers.** Three `TODO(keyring)`.
  - `atomic.rs:9` → a backlog entry. Acceptable.
  - `sessions.rs:19` → a backlog entry. Acceptable.
  - `vault/crypto.rs:130` (the cipher key schedule is not zeroized) has **no** tracking
    reference. Point it at a backlog entry, or record it with the `zeroize` ASK.
- **Fallbacks.** Refresh returns an empty key on a failed reopen. That is a recorded developer
  decision, but S3 narrows it. There are no silent defaults; `updated_at` is an explicit error.
- **Unused code.** `with_rotation_grace` (N7). `list`, `remove` and `unlock_slot_ids` are 4/9
  surface and acceptable.
- **Stale config comment.** `daemon.yaml.production` (see *Stale text*).

### From /analyze-clean-code

**Score: B.** New code has no must-refactor item.

- **Functions over 60 lines** are all pre-existing:
  - `build_auth_entries_admitting` (107, touched);
  - `pr_status_for_caller` (77, +3);
  - `mint_live_kit_token` (79, untouched).
- **Needs attention.** `refresh_session` is 41 lines (touched). The longest new functions are
  `retained_github_token` (40) and `SessionVaults::reopen` (38), both acceptable, with nesting at
  most 3.
- **Oversized: `tddy-github/src/auth_service.rs`, 1292 lines (622 production).** It grew about 260
  in this change.
  - **Split.** Move the vault glue into `auth_service/vault.rs`: `retain_the_login_credential`,
    `reopen_the_vault`, `caller_login`, `vaults_to_unlock`, `check_new_passphrase`,
    `to_proto_state`, `github_record`, `refused_by_the_vault`, and the `unlock_vault`/`reset_vault`
    bodies. That is about 220 lines.
  - **Cost.** Private helpers become `pub(super)`. No consumer repoints.
  - **Recommendation.** Split before 4/9 adds the Accounts RPCs here.
- **Oversized, but under 500 production lines.**
  - `tddy-credentials/src/sessions.rs` is 752 lines (342 production + 410 tests), new.
  - `vault.rs` is 698 lines (444 production).
  - Their size is test modules. Moving the tests to `tests/` would need `pub(crate)` seams
    (`SessionVaults.rotation_grace`, `wipe_text`). Leave as is.
- **Pre-existing and only touched** (report only): `auth.rs` 1400, `runtime.rs` 1680, `config.rs`
  2536, `connection_service.rs` 1652, `host_registry.rs` 1075. `useAuth.ts` is 520 (+38).
- **Magic values.** `MIN_PASSPHRASE_CHARS` is duplicated in TS (N4). Nothing else in new code:
  costs, nonce, salt, grace and slot bound are all named.
- **Duplication.**
  - `unlock_vault` and `reset_vault` share a 3-line prologue (take the passphrase, `caller_login`,
    `vaults_to_unlock`). Acceptable.
  - The test helper `decoded_parts` (see above).

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Publish the draft-PR contract — wave 2
- [x] M1–M12
- [x] Ask before taking `hkdf` / `zeroize` — neither taken
- [ ] Package documentation for the five affected packages
- [ ] `/wrap-context-docs` — this node claims **no** backlog entry and **no** code-issue record; it
      adds two backlog entries and two `oversized-file` records at wrap (see Affected Packages)

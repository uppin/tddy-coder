# Changeset: Session-gated encrypted credential store

**Date**: 2026-09-19
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Stack**: `#keyring` 3/9 · branch `feature/keyring/store` · base `feature/keyring/desktop-login` (#509)

## Affected Packages

- **tddy-credentials** (**new crate**): `docs/credential-store.md` — the record model, the sealed
  file format, key derivation, the session-scoped handle
- **tddy-github**: `src/token_store.rs` — **deleted**; `src/auth_service.rs` writes through the new
  store
- **tddy-daemon-auth**: [auth-service.md](../../../packages/tddy-daemon-auth/docs/auth-service.md)
  - `src/github_token_store.rs` — **deleted**
  - `src/auth.rs:83-152` — vault construction; the half-login rule extended to "cannot open"
- **tddy-daemon**: [daemon-endpoint.md](../../../packages/tddy-daemon/docs/daemon-endpoint.md)
  - `src/runtime.rs:882` — construction and injection. **The file is not split**
- **tddy-session-lifecycle**: [session-service.md](../../../packages/tddy-session-lifecycle/docs/session-service.md)
  - `src/connection_service/svc_pr_status_for_caller.rs:93` — the one external read, migrated

## Related Feature Documentation

- [PRD — Session-gated encrypted credential store](../../ft/daemon/1-WIP/PRD-2026-09-19-keyring-store.md)
- [Cross-daemon session authentication](../../ft/daemon/session-auth.md)
- [PR-stack live status](../../ft/coder/pr-stack-live-status.md)

## Summary

Replaces `GitHubTokenStore` — a two-method trait over a `0600` **plaintext** JSON file — with a
generic, provider-extensible credential store in a new crate, `tddy-credentials`, encrypted at rest
and openable only while a valid user session exists.

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
- key derivation — the login-derived KEK, the random data key it wraps, the verifier, re-wrapping
  on every login, and zeroization;
- the deletion of `GitHubTokenStore`, `FileGitHubTokenStore` and `github-tokens.json`;
- the extension of the half-login rule to cover a vault that cannot be opened.

## Boundaries

**Owned surface:**

| Symbol | Crate |
|---|---|
| `ProviderId`, `AccountId`, `CredentialRecord` | `tddy-credentials` (new) |
| `CredentialStore` (open / put / get / list / remove) | `tddy-credentials` |
| `SessionVault` — the opened, session-scoped handle | `tddy-credentials` |
| the on-disk format and its versioned header | `tddy-credentials` |

**Explicitly not this node's:**

- **The Accounts RPC service and screen** — `#keyring` 4/9. This node ships storage, not a UI, and
  exposes no new RPC.
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
This node needs a session that exists without a LiveKit secret, because the vault key is derived at
login. It does **not** depend on 2/9; both are wave 2, and 2/9 sits ahead of it in the line by the
developer's explicit instruction rather than by the sort (see `## Green wave`).

**A soft but real coupling to 2/9, stated so it is not lost**: 2/9 chooses an **OAuth App**, whose
user access token does not expire. That is what makes this node's derivation deterministic *across
logins* instead of a per-session accident. If that decision is reversed to a GitHub App — whose user
token expires in 8 hours — this node's key derivation must move to a credential that is stable, and
the choice is a re-plan, not an adjustment.

**Dependents**: 4/9 `accounts`, 6/9 `sync`, 7/9 `screen-share`, 8/9 `link-github` — six transitive.

**New external dependencies: none required.** `chacha20poly1305 0.10`, `argon2 0.5`, `sha2 0.10`,
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
impl CredentialStore {
    pub fn open_or_create(path: &Path, ikm: &[u8], subject: &str) -> Result<SessionVault, VaultError>;
}
pub struct SessionVault;         // zeroizes on drop
impl SessionVault {
    pub fn put(&self, record: CredentialRecord) -> Result<(), VaultError>;
    pub fn get(&self, provider: &ProviderId, account: &AccountId) -> Result<Option<CredentialRecord>, VaultError>;
    pub fn list(&self, provider: Option<&ProviderId>) -> Result<Vec<CredentialRecord>, VaultError>;
    pub fn remove(&self, provider: &ProviderId, account: &AccountId) -> Result<(), VaultError>;
    pub fn rewrap(&self, ikm: &[u8]) -> Result<(), VaultError>;
}
pub enum VaultError { Locked, FormatMismatch { .. }, Io(..), Crypto }
```

**Failing tests**

- a record written in one session opens in the next from the same derived key;
- the file's bytes contain no plaintext secret, label or metadata;
- tampering with a record's `label` or `metadata` fails the open (AEAD covers the whole record);
- a changed KDF parameter surfaces as `FormatMismatch`, not a failed decrypt;
- a different input keying material yields `Locked` — and **nothing is re-initialised**;
- `rewrap` succeeds and the vault opens under the new key and not the old;
- key material is zeroized on drop;
- acceptance: a failed credential write fails the login; so does a `Locked` vault;
- acceptance: PR-stack live status resolves `Empty` / `Unavailable(reason)` / `Perform(token)`
  exactly as before through the new read path.

⚠ **Not mergeable in that state** — implementation follows in this same PR.

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

### Unanalyzed packages

`tddy-github` was analyzed on 1/9 (one record, claimed by 2/9). `tddy-session-lifecycle` (22
records), `tddy-daemon` (3) and `tddy-daemon-auth` (1) are analyzed; none of their records outside
the two above is in this node's path. `tddy-credentials` is new, so there is nothing to have
analyzed.

## Scope

- [x] **PRD**: [PRD-2026-09-19-keyring-store.md](../../ft/daemon/1-WIP/PRD-2026-09-19-keyring-store.md)
- [x] **Changeset**: this document
- [ ] **Draft PR contract**: owned surface + failing tests (wave 2, commit 2)
- [ ] **New crate**: `tddy-credentials` — record model, sealed format, session handle
- [ ] **Key derivation**: KEK from the login credential, wrapped data key, verifier, re-wrap, zeroize
- [ ] **Deletions**: `token_store.rs`, `github_token_store.rs`, every `github-tokens.json` reference
- [ ] **Migration**: `svc_pr_status_for_caller.rs:93`
- [ ] **Dependency approvals** (if taken): `hkdf`, `zeroize`
- [ ] **Testing**: unit + acceptance, scoped to the five packages
- [ ] **Package Documentation**: the five packages above
- [ ] **Code Quality**: scoped clippy; CI green

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
- The data key is random and wrapped under a KEK derived at login from the user's own credential.
  At rest the file holds ciphertext and a wrapped key; nothing in it opens it.
- `SessionVault` lives for the life of a session and zeroizes on drop.
- `github-tokens.json` does not exist and is never read.

### Delta (What's Changing)

#### tddy-credentials (new)
- **Architecture**: depends on neither the auth crate nor LiveKit — deliberately, so 6/9 and 7/9 can
  use it directly.
- **API**: `CredentialStore::open_or_create`, `SessionVault::{put,get,list,remove,rewrap}`,
  `VaultError::{Locked, FormatMismatch, Io, Crypto}`.
- **Implementation**: ChaCha20-Poly1305 per record with the record's identity as associated data;
  HKDF-SHA256 over the login credential; a verifier ciphertext; `write_atomic_with_mode(…, 0o600)`.

#### the locked case
`VaultError::Locked` is returned when the derived KEK does not unwrap the data key — the user
revoked authorisation and re-approved, so GitHub issued a different token. The daemon reports it
**distinctly** and the user re-links their accounts into a fresh vault. It does **not** re-initialise
silently and there is **no second key**. Mitigation: every successful login calls `rewrap`, so a
rotation observed while a session can still be established costs nothing; only a rotation with no
live session and no old credential costs the vault.

#### tddy-github
- **API**: `token_store.rs` deleted. `auth_service.rs` writes a `CredentialRecord`.

#### tddy-daemon-auth
- **Implementation**: `github_token_store.rs` deleted; `auth.rs:83-152` opens the vault at login and
  re-wraps; the half-login rule now also fires on `Locked`.

#### tddy-daemon
- **Implementation**: `runtime.rs:882` constructs and injects the store. **Not split.**

#### tddy-session-lifecycle
- **Integration**: `svc_pr_status_for_caller.rs:93` reads through `SessionVault`.
  `pr_lookup_for_caller`'s three outcomes are unchanged.

## Implementation Milestones

- [ ] **M1** — `tddy-credentials`: record model, versioned header, seal/open, `write_atomic_with_mode`
- [ ] **M2** — key derivation, wrapped data key, verifier, `rewrap`, zeroization
- [ ] **M3** — wire construction in `auth.rs` and `runtime.rs`; derive on login, re-wrap on login
- [ ] **M4** — migrate `svc_pr_status_for_caller.rs:93`
- [ ] **M5** — delete `token_store.rs`, `github_token_store.rs`, every `github-tokens.json` reference
- [ ] **M6** — extend the half-login rule to `Locked`
- [ ] **M7** — `tddy-credentials` documentation, carrying the two retention rules forward

## Testing Plan

### Testing Strategy

**Two levels, and the split is principled.** The file format and key derivation are pure
input/output over bytes and a temp directory — **unit**, where an assertion can look at the actual
ciphertext. The rules that matter to a user — a failed write fails the login, a locked vault fails
the login, PR status behaves identically — are properties of the wired daemon, so **acceptance**.

### Unit tests (`tddy-credentials`)

- A record written and re-opened from the same derived key round-trips exactly.
- **The file's bytes contain no plaintext secret, label or metadata.** Asserted against the file
  contents, not inferred from the API.
- Flipping a byte in a record's sealed label or metadata fails the open — the AEAD covers the whole
  record, which is the limit being fixed relative to `screen_sharing_vault.rs`.
- A changed KDF parameter in the header surfaces as `FormatMismatch`, not as a failed decrypt.
- Different input keying material yields `Locked`, **and the file is unchanged afterwards**.
- `rewrap` succeeds; the vault then opens under the new key and **not** under the old one.
- Key material is zeroized on drop.

### Acceptance tests

- **A failed credential write fails the login** — the existing half-login rule, now over the vault.
- **A `Locked` vault fails the login**, with the lock reported distinctly rather than as a generic
  auth failure.
- **PR-stack live status is behaviour-identical** — Given a stored token, a stub-mode daemon, and a
  daemon with no stored token, Then `Perform(token)`, `Empty` and `Unavailable(reason)` resolve
  exactly as before.

### Verification scope

`./test -p tddy-credentials -p tddy-github -p tddy-daemon-auth -p tddy-daemon -p tddy-session-lifecycle`
and scoped clippy per package. Whole-workspace green comes from CI via `scripts/ci-status.sh`.

## Acceptance Criteria

- [ ] A credential is readable in-session and **unreadable from the file alone**
- [ ] `label` and `metadata` are inside the AEAD — tampering with either fails the open
- [ ] The header carries KDF name, version and parameters; a change is a detected `FormatMismatch`
- [ ] A second login by the same user opens the same vault
- [ ] A changed credential yields `Locked`, reported distinctly, with **no re-initialisation and no
      second key**
- [ ] Every successful login re-wraps the data key
- [ ] A failed write fails the login; a `Locked` vault fails the login
- [ ] No store API returns a secret to an RPC response path
- [ ] Key material is zeroized on drop and not cached beyond a session
- [ ] `GitHubTokenStore`, `FileGitHubTokenStore` and all `github-tokens.json` references are gone
- [ ] PR-stack live status behaves identically

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [ ] Publish the draft-PR contract — wave 2
- [ ] M1–M7
- [ ] Ask before taking `hkdf` / `zeroize`
- [ ] Package documentation for the five affected packages
- [ ] `/wrap-context-docs` — this node claims **no** backlog entry and **no** code-issue record

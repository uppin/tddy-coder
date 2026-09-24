# Session-gated encrypted credential store - PRD

**Date**: 2026-09-19
**PRD Type**: Architecture Change
**Stack**: `#keyring` 3/9 — PR #510, based on `master`; 1/9 `signing-key` (#508) and 2/9 `desktop-login` (#509) are merged

## Affected Features

- **Primary**: [Cross-daemon session authentication](../session-auth.md) — its § *GitHub access-token
  retention* describes a plaintext file. After this node the token lives in an encrypted vault that
  only a valid session can open, and the section's security rule is enforced by construction rather
  than by convention.
- **Related**: [PR-stack live status](../../coder/pr-stack-live-status.md) — the one feature outside
  the auth crate that reads a retained GitHub token. Its read path changes; its behaviour does not.
- **Related**: [Screen-sharing sessions](../../web/screen-sharing-sessions.md) — this node
  establishes the vault that `#keyring` 7/9 folds screen-sharing secrets into. Nothing in that
  feature changes here.

## Summary

Replaces `GitHubTokenStore` — a two-method trait over a `0600` **plaintext** JSON file — with a
**generic, provider-extensible credential store** in a new crate, `tddy-credentials`, whose contents
are encrypted at rest and decryptable only by their owner.

The store holds `(provider, account, secret)` records rather than `login → token` pairs, which is
what lets `cloudflare` and a second GitHub account exist later without another storage format. The
encryption key is derived from the user's **vault passphrase**, which the daemon never stores, so a
daemon at rest — or a backup of its data directory — holds ciphertext and nothing that opens it.

**Revised 2026-09-23.** The first version of this PRD derived the key from the GitHub access token
a login produced. That premise was wrong: a GitHub OAuth App mints a **new** token at every code or
device exchange, so after a daemon restart any fresh login derived a key that did not open the vault
and was refused. The developer chose a user passphrase as the stable secret; the sections below
describe that design.

Breaking: `GitHubTokenStore`, `FileGitHubTokenStore` and `github-tokens.json` are **deleted**, not
deprecated. No fallback path reads the old file.

## Background

The surface being replaced is small, which is what makes replacing it outright reasonable.

`packages/tddy-github/src/token_store.rs:15` is a two-method trait — `put(login, access_token)` and
`get(login)`. Its only implementation, `packages/tddy-daemon-auth/src/github_token_store.rs`, is one
`0600` JSON file holding a `HashMap<String, String>` of login → token, written through
`write_atomic_with_mode` and guarded by a process-wide `static PUT_LOCK: Mutex<()>` because
concurrent logins would otherwise clobber each other.

Its **only reader outside the auth crate** is the PR-status read, now
`packages/tddy-daemon-rpc/src/pr_stack/pr_status.rs:56` (it was `svc_pr_status_for_caller.rs:93` in `tddy-session-lifecycle` before
`#carve` moved it). Everything else is construction and plumbing (`auth.rs:168-275`,
`runtime.rs:1076`).

Three properties of that trait are why it cannot be extended in place:

1. **The 2-argument signature has no room for anything else.** No refresh token, no provider
   dimension, no account identity beyond a login string. Every later node in this stack needs at
   least one of those.
2. **The secret is at rest in plaintext.** `0600` protects it from other users on the host; it
   protects it from nothing that reads the disk — a backup, a snapshot, a stolen laptop, a
   misconfigured sync.
3. **Its doc comment states a rule the replacement must keep.** The GitHub token is deliberately
   held *outside* the session token, because that token goes to a browser over a plain-http LAN
   origin, so a live `repo`-scoped credential must never travel in it or be returned to the client.
   The same comment states that a failed `put` must **fail the login** — "a session minted without
   its token is a half-login".

**The encryption pattern already exists in this repo**, and so do all but one of the crates it
needs. `packages/tddy-screen-sharing/src/screen_sharing_vault.rs` (394 lines) derives a key with
Argon2id, seals items with ChaCha20-Poly1305 under a fresh nonce, proves a passphrase with a
`VERIFIER_PLAINTEXT` ciphertext rather than storing the key, and writes through
`write_atomic_with_mode(path, …, 0o600)`. Four of its limits are to be **fixed rather than copied**,
and they are listed under *What's Changing*.

## Proposed Changes

### What's Changing

**A new crate, `tddy-credentials`.** It owns the record model and the sealed file, and depends on
neither the auth crate nor the LiveKit crate, so `#keyring` 6/9 can synchronise it and
`#keyring` 7/9 can store a different provider's secret in it without either reaching through auth.

```
ProviderId       "github" | "cloudflare" | "screen-sharing" | …  (open, not an enum)
AccountId        stable, opaque, daemon-minted — not a login, which can be renamed
CredentialRecord { provider, account, label, secret, metadata, updated_at }
```

`label` is what a human sees in the Accounts screen (`#keyring` 4/9); `metadata` carries
provider-specific non-secret fields (a GitHub numeric id, an avatar URL). **Both are inside the
AEAD**, not beside it — see below.

**The at-rest format**, with the four known limits of the existing vault fixed:

| Limit in `screen_sharing_vault.rs` | Fixed how |
|---|---|
| Only the password field is AEAD'd, so item metadata has **no integrity protection** | The whole record — label, metadata, secret — is one sealed unit, with the record's identity as associated data |
| `argon2::Argon2::default()` parameters are not versioned on disk | KDF name, version and parameters are written into the file header and read back, so a parameter change is a format migration rather than silent corruption |
| The passphrase crosses the wire in plaintext (`req.passphrase`, `:862`) | **Not fixed, and stated.** The vault passphrase is sent in `UnlockVault` / `ResetVault`, over the same origin the session token uses; it is never logged, stored or returned. (The first version of this PRD avoided a passphrase entirely by deriving from the login token, which does not work — see the Summary) |
| `DerivedKey(pub [u8; 32])` is never zeroized and is cached in a `HashMap` keyed by `session_id` | Key material is zeroized on drop and held for the life of a session, not in a map that outlives one |

**Key derivation — the part worth arguing about.** The requirement is that the store is openable
only by its owner, with no daemon-held key. Nothing a login carries is stable — GitHub mints a new
token per exchange — so the one stable secret is one the user holds: a **vault passphrase**.

```
DK    = 32 random bytes, generated once — the key every record is sealed under
KEK   = HKDF-Expand(Argon2id(passphrase, salt, m=19456 KiB, t=2, p=1),
                    info = "tddy-credentials/v2/passphrase/" || <subject>)
disk  = { header{format 2, argon2id v19, m, t, p, salt}, wrapped_DK = AEAD(KEK, DK, aad = header),
          unlock_slots[], verifier, records[] }
```

The GitHub token is a **record** in the vault, never key material.

**A login always signs in, and says where the vault stands** — `VaultState` on the login, refresh
and status responses:

| State | What happened to this login's GitHub token | What the page does |
|---|---|---|
| `OPEN` — open on this daemon | sealed at once; the lineage gets an unlock slot | nothing |
| `LOCKED` — a file exists, closed since the daemon started | held in memory, never written in plaintext | asks for the passphrase |
| `UNINITIALIZED` — no vault yet | held in memory | asks the user to choose a passphrase |
| `NONE` — stub login, or no `auth_storage` | not retained | nothing |

`UnlockVault(session_token, passphrase, create)` opens the vault (or creates it, with `create`),
seals the waiting token, adds an unlock slot and returns its key. A wrong passphrase is
`failed_precondition` naming the lock, and changes nothing on disk. PR status, meanwhile, reads
*unavailable* with a reason saying to unlock the credential vault.

> **The failure mode is deliberate and has no fallback.** A forgotten passphrase is not recoverable
> by the daemon: there is no daemon-held master key, because one would let the daemon read the
> vault with nobody present — the property this node exists to remove. The remedy is explicit:
> `ResetVault(session_token, new_passphrase)` renames the old file aside
> (`credentials-<hex>.locked-<unix>.vault`, **never deleted** — the old passphrase still opens it)
> and creates a fresh, empty vault, sealing the waiting token into it. Every other credential must
> be linked again.

**A daemon restart need not ask for the passphrase** (added at green, by the developer's decision).
The daemon still holds no key; the **browser** does. The data key is wrapped by more than one slot:
the passphrase slot above, and one **unlock slot** per browser session lineage that opened the
vault.

```
U            = 32 random bytes, minted per login and returned as `vault_unlock_key`
unlock KEK   = HKDF-SHA256(ikm = U, info = "tddy-credentials/v2/unlock/" || slot id || "/" || subject)
unlock slot  = AEAD(unlock KEK, DK)       — stored in the vault; U is not
```

- **Login** returns `U` beside the refresh token (`ExchangeCodeResponse.vault_unlock_key`, and
  `PollDeviceLoginResponse` on `COMPLETE`) when the vault is `OPEN`; otherwise `UnlockVault` returns
  it. A stub login returns none and creates nothing.
- **RefreshSession** takes `U` back. When the vault is not open — the daemon restarted — the daemon
  opens it through that slot and registers it; either way it **rotates** the slot and returns `U'`,
  so the presented key opens nothing afterwards. A key that does not open its slot is logged; the
  refresh still succeeds with an empty key and the vault's state. The rotation proves `U` under the
  write lock, and for 30 s the key a refresh just retired is answered with the same `U'`, so two
  tabs sharing one stored key both keep a working one.
- **Logout** takes `U` and removes that lineage's slot; the last lineage's logout closes the vault
  on the daemon.
- At most **16** slots per vault; the least recently used is evicted.
- Between a restart and the first refresh, PR status is *unavailable* with a reason saying to unlock
  the credential vault, or that it reopens at the next refresh — never "sign in again". The web
  client refreshes on page load when it holds `U`.

**The trade-off, stated honestly.** `U` plus a copy of the disk is the plaintext, and so is the
passphrase plus a copy of the disk. `U` crosses the plain-http LAN origin at every login, refresh
and logout and sits in `localStorage` beside the refresh token; the passphrase crosses the same
origin in `UnlockVault` / `ResetVault`. Rotation bounds a copied `U` against the *live* file only:
an old backup keeps the slots it was taken with. What the design buys is that a disk **without**
either is ciphertext, including every backup of it, and that a daemon at rest holds no key.

**One vault per user.** `auth_storage/credentials-<hex login>.vault` — a vault opens for one subject
only, so one shared file would lock out every user after the first.

**`GitHubTokenStore` and `FileGitHubTokenStore` are deleted.** The one external reader,
`packages/tddy-daemon-rpc/src/pr_stack/pr_status.rs:56`, moves to the new store's read path. `github-tokens.json` is not
read, not migrated, and not renamed: on first login after this change the vault is created and the
token stored afresh. That is one re-login, and `#keyring` 1/9 already ends every session once.

**The half-login rule is kept, and restated.** A login whose token cannot be retained is
**reported, never silent**: a closed vault is `LOCKED` or `UNINITIALIZED` in the response, the page
asks for the passphrase, and PR status names the remedy. A failed write still fails the login.

### What's Staying the Same

- **The GitHub token never travels in the session token** and is never returned to the client. The
  rule `token_store.rs` states in prose becomes a property of the store's API: nothing on it returns
  a secret to an RPC response path.
- **`0600` and an atomic replace** — same at-rest mode, same swap-then-rename, same reason (the swap
  file's mode is set as it is created rather than copied from a target that may not exist). The
  writer is inlined in `tddy-credentials` rather than taken from `tddy-core`, so the crate does not
  pull in the workflow stack; the duplication is a recorded follow-up.
- **PR-stack live status behaviour.** `pr_lookup_for_caller`'s three outcomes — `Empty`,
  `Unavailable(reason)`, `Perform(token)` — are unchanged, including the stub path that resolves to
  a clean empty result rather than an error.
- **`StubGitHubProvider`'s synthetic token is still never retained**, by the same
  `issues_usable_access_token()` check.
- **Screen sharing**, untouched here. `#keyring` 7/9 folds it in.

## Impact Analysis

### Technical Impact

| Package | Change |
|---|---|
| **`tddy-credentials`** (new) | the record model, the sealed file, key derivation, the session-scoped handle |
| `tddy-github` | `token_store.rs` **deleted**; `auth_service.rs` writes through the new store |
| `tddy-daemon-auth` | `github_token_store.rs` **deleted**; `auth.rs:168-275` constructs the vault registry; PR-status reads by vault state |
| `tddy-daemon` | `runtime.rs:1076` — the store's construction and injection |
| `tddy-session-lifecycle` | `handler_state.rs:68` — `DaemonSessionHost::credential_vaults()`, which hands the registry to the PR-stack handler |
| `tddy-daemon-rpc` | `pr_stack/pr_status.rs:56` — the one external read, migrated; a path dependency on `tddy-credentials` |
| `tddy-service` | `auth.proto` — additive `vault_unlock_key` fields on five messages, a `VaultState` enum on four, and **two new RPCs**, `UnlockVault` and `ResetVault` |
| `tddy-web` | the unlock key stored beside the refresh token, presented on refresh, sent on logout; a passphrase prompt for a `LOCKED` or `UNINITIALIZED` vault |

**New external dependencies: none taken.** `argon2 0.5` (already in the workspace) is added to
`tddy-credentials`; `chacha20poly1305 0.10`, `sha2 0.10`,
`hmac 0.12`, `subtle 2.6` and `rand` are already in the workspace, and HKDF-Extract/Expand is a
dozen lines over the existing `hmac` + `sha2`. Two crates would be tidier and **both need approval
under CLAUDE.md § ASK** before they are used: `hkdf 0.12` (in place of the hand-rolled expand) and
`zeroize` (for `DK` and `KEK`). The design does not depend on either being granted.

**A crate boundary chosen deliberately.** `tddy-credentials` depends on neither `tddy-daemon-auth`
nor `tddy-livekit`. `#keyring` 6/9 synchronises this file between daemons and `#keyring` 7/9 stores a
different provider's secret in it; both would otherwise have to reach through the auth crate, and
`packages/tddy-daemon-kernel`'s own code-issue record shows what a single misplaced dependency costs
a crate's dependents.

**Unanalyzed package in this node's path**: `tddy-github` was analyzed on `#keyring` 1/9 (one record,
claimed by 2/9). No other package this node touches is unanalyzed.

### User Impact

- **Breaking: one re-login.** `github-tokens.json` is not migrated, so the first login after this
  change re-establishes the stored token. `#keyring` 1/9 already ends every session once, so for
  anyone taking the stack in order this costs nothing additional.
- **A stolen disk stops being a stolen credential.** A backup, a snapshot or a copied data directory
  now holds ciphertext, and the key is not in it.
- **A daemon at rest cannot act as the user.** After a restart, the daemon holds no key that opens
  the vault until a browser's refresh presents its unlock key or the user gives the passphrase.
- **One new thing to remember: a vault passphrase.** Chosen once, at the first real login. A fresh
  browser after a restart asks for it; a browser that has unlocked before does not.
- **A forgotten passphrase costs the stored credentials, not the file.** A reset sets the old vault
  aside and starts an empty one; credentials must be linked again. No silent re-initialisation and
  no second way in.

## Implementation Plan

1. `tddy-credentials`: record model, file format with a versioned header, seal/open, the
   session-scoped handle, zeroization.
2. Key derivation — the passphrase KEK and the wrapped-`DK` envelope, with the verifier and the
   browser unlock slots.
3. Wire construction in `auth.rs` and `runtime.rs`; retain a login's token by vault state; the
   `VaultState` fields, `UnlockVault` and `ResetVault`.
4. Migrate the PR-status read (`packages/tddy-daemon-rpc/src/pr_stack/pr_status.rs:56`) to the new read path.
5. Delete `token_store.rs`, `github_token_store.rs` and every reference to `github-tokens.json`.
6. The web passphrase prompt.

**Verification is scoped** (`./test -p tddy-credentials -p tddy-github -p tddy-daemon-auth
-p tddy-daemon -p tddy-session-lifecycle`). Whole-workspace green comes from CI.

## Acceptance Criteria

- [x] A credential written under one provider/account is readable in the same session and
      **unreadable from the file alone** — the bytes on disk contain no plaintext secret and no
      passphrase
- [x] A record's `label` and `metadata` are inside the AEAD: tampering with either fails the open
- [x] The file header carries the KDF name, version and parameters, and a changed parameter is a
      detected format mismatch rather than a failed decrypt
- [x] After a daemon restart, a fresh login that receives a **different** GitHub token is signed in,
      told the vault is `LOCKED`, and opens **the same vault** once the passphrase is given
- [x] The first real login creates the vault under a passphrase the user chooses
- [x] A wrong passphrase is `failed_precondition` naming the lock; the file is unchanged and the
      session stays signed in, reporting the vault locked
- [x] A login while the vault is open needs no passphrase; the new lineage just gets an unlock slot
- [x] A reset renames the old vault aside — never deletes it — and seals the login's token into a
      fresh one under the new passphrase
- [x] A failed credential write **fails the login**; a closed vault is reported, never silent
- [x] No API on the store returns a secret to an RPC response path; no passphrase reaches a log,
      the vault file or any response
- [ ] Key material is zeroized on drop and is not cached beyond the session's life — ✅ dropped at
      the last logout; ⚠ a lineage that lapses without a logout keeps the vault open until the
      daemon exits (`docs/dev/todo/2026-09-23-credential-vault-open-past-its-last-session.md`)
- [x] A vault sealed for one user does not open for another, even with the same passphrase — the
      subject is bound into the derivation
- [x] **A stub login seals nothing**: it succeeds, reports no vault, and leaves no vault file behind
- [ ] `GitHubTokenStore`, `FileGitHubTokenStore` and every reference to `github-tokens.json` are
      **gone** — no fallback read. ✅ code and config; ⚠ docs owed at wrap
- [x] PR-stack live status behaves identically: `Empty`, `Unavailable(reason)` and `Perform(token)`
      resolve exactly as before, and a closed vault's reason says to unlock it
- [x] After a daemon restart, a session refresh presenting the unlock key reopens the vault and PR
      lookups perform with the stored token — **with no passphrase and no new login**
- [x] The unlock key rotates on every refresh; the presented one no longer opens its slot, yet two
      tabs sharing one key both keep a working one
- [x] Logout removes the lineage's unlock slot
- [x] The vault file never contains an unlock key; a stub login is handed none
- [x] Choosing a vault passphrase — a first one, or a reset — needs a fresh GitHub sign-in on this
      daemon; an access token alone cannot, a reset is refused while the vault is open, and old
      vaults set aside are capped with nothing ever deleted (`credential_vault_guard_acceptance.rs`)
- [x] Wrong passphrases are throttled per user and say when to retry, and no passphrase derivation
      runs on an RPC worker (`credential_vault_guard_acceptance.rs`, `auth_service/vault/backoff.rs`)
- [x] A refresh that cannot read the vault keeps the browser's unlock key
      (`vault_unlock_across_restart_acceptance.rs`, `sessionTokenStore.test.ts`)

## References

### Affected Features (Complete List)

- [Cross-daemon session authentication](../session-auth.md) — § GitHub access-token retention
- [PR-stack live status](../../coder/pr-stack-live-status.md) — the one external reader
- [Screen-sharing sessions](../../web/screen-sharing-sessions.md) — the vault 7/9 folds in

### Stack

`#keyring` 3/9. Parents: 1/9 `signing-key` (#508) and 2/9 `desktop-login` (#509), both merged. Dependents: 4/9 `accounts` (the service and screen over
this store), 6/9 `sync` (propagates this file), 7/9 `screen-share` (becomes a provider in it),
8/9 `link-github` (a second account row).

### Backlog

- ⚠ [2026-07-26 — PR-stack status polling and stack hygiene](../../../dev/todo/2026-07-26-pr-stack-status-polling-and-stack-hygiene.md)

### Code issues

- ⚠ [`complexity-runtime-build`](../../../../packages/tddy-daemon/docs/code-issues/complexity-runtime-build.md) — `runtime.rs:623` (`build`), covering the `:1076` store wiring. Recorded, **not claimed**; this node must not split it

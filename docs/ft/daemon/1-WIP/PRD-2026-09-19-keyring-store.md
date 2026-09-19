# Session-gated encrypted credential store - PRD

**Date**: 2026-09-19
**PRD Type**: Architecture Change
**Stack**: `#keyring` 3/9 — depends on 1/9 `signing-key`

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
are encrypted at rest and decryptable only while a valid user session exists.

The store holds `(provider, account, secret)` records rather than `login → token` pairs, which is
what lets `cloudflare` and a second GitHub account exist later without another storage format. The
encryption key is derived from the credential the user's own login produced, so a daemon at rest —
or a backup of its data directory — holds ciphertext and nothing that opens it.

Breaking: `GitHubTokenStore`, `FileGitHubTokenStore` and `github-tokens.json` are **deleted**, not
deprecated. No fallback path reads the old file.

## Background

The surface being replaced is small, which is what makes replacing it outright reasonable.

`packages/tddy-github/src/token_store.rs:15` is a two-method trait — `put(login, access_token)` and
`get(login)`. Its only implementation, `packages/tddy-daemon-auth/src/github_token_store.rs`, is one
`0600` JSON file holding a `HashMap<String, String>` of login → token, written through
`write_atomic_with_mode` and guarded by a process-wide `static PUT_LOCK: Mutex<()>` because
concurrent logins would otherwise clobber each other.

Its **only reader outside the auth crate** is
`packages/tddy-session-lifecycle/src/connection_service/svc_pr_status_for_caller.rs:93`. Everything
else is construction and plumbing (`auth.rs:83-152`, `runtime.rs:882`).

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
| The passphrase crosses the wire in plaintext (`req.passphrase`, `:862`) | **No passphrase crosses any wire here.** The key is derived daemon-side from a credential the session already established |
| `DerivedKey(pub [u8; 32])` is never zeroized and is cached in a `HashMap` keyed by `session_id` | Key material is zeroized on drop and held for the life of a session, not in a map that outlives one |

**Key derivation — the part worth arguing about.** The requirement is that the store is "only
decrypted by a valid user session", with "user login producing a deterministic encryption key". The
design that satisfies both without a fallback:

```
DK    = 32 random bytes, generated once — the key every record is sealed under
KEK   = HKDF-SHA256(ikm = the GitHub access token, salt = per-vault random salt,
                    info = "tddy-credential-vault-v1/" || <github user id>)
disk  = { header{kdf, params, salt}, wrapped_DK = AEAD(KEK, DK), verifier, records[] }
```

A login hands the daemon the user's access token; the daemon derives `KEK`, unwraps `DK`, and holds
`DK` for the life of the session. At rest the file holds `wrapped_DK` and ciphertext — nothing that
opens it. `#keyring` 2/9's choice of an **OAuth App**, whose user token does not expire, is what
makes the derivation deterministic across logins rather than a per-session accident; that dependency
is stated here because it is easy to lose.

> **The failure mode is deliberate and has no fallback.** If GitHub issues a *different* token — the
> user revoked authorisation and re-approved — `KEK` no longer unwraps `DK`, and the vault cannot be
> opened. The daemon reports that distinctly (`vault_locked: credential changed`) and the user
> re-links their accounts into a fresh vault. It does **not** silently re-initialise, and it does
> **not** keep a daemon-held master key as a second way in. A master key would make the daemon able
> to read the vault without a user present, which is exactly the property this node exists to
> remove.
>
> The mitigation, which is cheap: every successful login **re-wraps `DK`** under a freshly derived
> `KEK`. A rotation observed while a session can still be established is therefore survivable; only
> a rotation that happens with no live session and no old token costs the vault.

**`GitHubTokenStore` and `FileGitHubTokenStore` are deleted.** The one external reader,
`svc_pr_status_for_caller.rs:93`, moves to the new store's read path. `github-tokens.json` is not
read, not migrated, and not renamed: on first login after this change the vault is created and the
token stored afresh. That is one re-login, and `#keyring` 1/9 already ends every session once.

**The half-login rule is kept and strengthened.** A failed write still fails the login. It now also
fails when the vault cannot be *opened*, because a session that cannot reach its own credentials is
the same half-login by a different route.

### What's Staying the Same

- **The GitHub token never travels in the session token** and is never returned to the client. The
  rule `token_store.rs` states in prose becomes a property of the store's API: nothing on it returns
  a secret to an RPC response path.
- **`0600` and `write_atomic_with_mode`** — same at-rest mode, same atomic replace, same reason
  (`write_atomic_with_mode` sets the swap file's mode before writing rather than copying it from a
  target that may not exist).
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
| `tddy-daemon-auth` | `github_token_store.rs` **deleted**; `auth.rs:83-152` constructs the vault; the half-login rule extended to "cannot open" |
| `tddy-daemon` | `runtime.rs:882` — the store's construction and injection |
| `tddy-session-lifecycle` | `svc_pr_status_for_caller.rs:93` — the one external read, migrated |

**New external dependencies: none required.** `chacha20poly1305 0.10`, `argon2 0.5`, `sha2 0.10`,
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
- **A daemon at rest cannot act as the user.** Between sessions, the daemon holds no key that opens
  the vault. That is the point, and it is also the constraint: work that needs a user's credential
  needs a live session.
- **Revoking GitHub authorisation locks the vault**, reported plainly, recovered by re-linking. No
  silent re-initialisation and no second way in.

## Implementation Plan

1. `tddy-credentials`: record model, file format with a versioned header, seal/open, the
   session-scoped handle, zeroization.
2. Key derivation and the wrapped-`DK` envelope, with the verifier.
3. Wire construction in `auth.rs` and `runtime.rs`; derive on login; re-wrap on every login.
4. Migrate `svc_pr_status_for_caller.rs:93` to the new read path.
5. Delete `token_store.rs`, `github_token_store.rs` and every reference to `github-tokens.json`.
6. Extend the half-login rule to cover "vault cannot be opened".

**Verification is scoped** (`./test -p tddy-credentials -p tddy-github -p tddy-daemon-auth
-p tddy-daemon -p tddy-session-lifecycle`). Whole-workspace green comes from CI.

## Acceptance Criteria

- [ ] A credential written under one provider/account is readable in the same session and
      **unreadable from the file alone** — the bytes on disk contain no plaintext secret
- [ ] A record's `label` and `metadata` are inside the AEAD: tampering with either fails the open
- [ ] The file header carries the KDF name, version and parameters, and a changed parameter is a
      detected format mismatch rather than a failed decrypt
- [ ] A second login by the same user derives the same key and opens the same vault
- [ ] A login whose credential has changed reports `vault_locked` **distinctly**, and does not
      re-initialise, overwrite, or open by any other means
- [ ] Every successful login re-wraps the data key
- [ ] A failed credential write **fails the login**; so does a vault that cannot be opened
- [ ] No API on the store returns a secret to an RPC response path
- [ ] Key material is zeroized on drop and is not cached beyond the session's life
- [ ] `GitHubTokenStore`, `FileGitHubTokenStore` and every reference to `github-tokens.json` are
      **gone** — no fallback read
- [ ] PR-stack live status behaves identically: `Empty`, `Unavailable(reason)` and `Perform(token)`
      resolve exactly as before

## References

### Affected Features (Complete List)

- [Cross-daemon session authentication](../session-auth.md) — § GitHub access-token retention
- [PR-stack live status](../../coder/pr-stack-live-status.md) — the one external reader
- [Screen-sharing sessions](../../web/screen-sharing-sessions.md) — the vault 7/9 folds in

### Stack

`#keyring` 3/9. Parent: 1/9 `signing-key`. Dependents: 4/9 `accounts` (the service and screen over
this store), 6/9 `sync` (propagates this file), 7/9 `screen-share` (becomes a provider in it),
8/9 `link-github` (a second account row).

### Backlog

- ⚠ [2026-07-26 — PR-stack status polling and stack hygiene](../../../dev/todo/2026-07-26-pr-stack-status-polling-and-stack-hygiene.md)

### Code issues

- ⚠ [`complexity-runtime-build`](../../../../packages/tddy-daemon/docs/code-issues/complexity-runtime-build.md) — `runtime.rs:498`, 806 lines, covering the `:882` store wiring. Recorded, **not claimed**; this node must not split it

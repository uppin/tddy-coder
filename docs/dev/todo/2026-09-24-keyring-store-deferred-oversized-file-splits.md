# 2026-09-24 — split the five over-budget files `#keyring` 3/9 grew, after the stack lands

**Category:** Deferred from `#keyring` 3/9 `store` (#510)
**Source:** `/pr-wrap` file-length gate on #510, range `origin/master..HEAD` (merge-base
`35cf2913`), production lines counted to the first `#[cfg(test)]` (the whole file when it has none)
**Consent:** the developer deferred all five splits during #510's wrap, 2026-09-24 — `auth.rs` and
`attach.rs` first, then `config.rs`, `runtime.rs` and `build.rs`

#510 grew five files that end over the 500-production-line budget. None is decomposed in #510.

| File | Production lines (`origin/master` → #510) | What #510 added | Record |
|---|---|---|---|
| `packages/tddy-daemon-auth/src/auth.rs` | 576 → 622 (unchanged by the idle-vault follow-up) | the credential-vault construction over `auth_storage` (`vault_lifetimes::credential_vaults_in`, formerly `pending_logins`) and the half-login rule extended to "cannot open" | `packages/tddy-daemon-auth/docs/code-issues/oversized-file-auth.md` |
| `packages/tddy-session-sync/src/attach.rs` | 517 → 520 | `RefreshSessionRequest.vault_unlock_key: String::new()` — a tool presents no unlock key | `packages/tddy-session-sync/docs/code-issues/oversized-file-attach.md` |
| `packages/tddy-daemon-kernel/src/config.rs` | 1,472 → 1,474 (1,476 after the idle-vault follow-up) | the `github.pending_login_ttl_seconds` and `github.open_vault_idle_ttl_seconds` fields, plain `Option<u64>`; their defaults and ceilings are `tddy-daemon-auth`'s `vault_lifetimes.rs` | `packages/tddy-daemon-kernel/docs/code-issues/oversized-file-config.md` |
| `packages/tddy-daemon/src/runtime.rs` | 1,619 → 1,620 | the credential sweep's spawn (pending sign-ins, and since the follow-up idle open vaults), beside the vaults' injection | `packages/tddy-daemon/docs/code-issues/oversized-file-runtime.md` |
| `packages/tddy-service/build.rs` | 692 → 695 | prost `skip_debug` for the two requests that carry a vault passphrase (N1), and its comment | `packages/tddy-service/docs/code-issues/oversized-file-build.md` |

**Why after the stack.**

- `auth.rs`: two stack parents (#508, #509) edited it; they are merged now, but #511 still touches
  it, and a mechanical move inside a credential-store PR buries the security review.
- `attach.rs`, `config.rs`, `runtime.rs`, `build.rs`: each was already over budget on
  `origin/master`, and #510 adds one to three lines — a field, a spawn, a redaction. A split would
  put an unrelated move of hundreds of lines in a credential-store diff, and `config.rs` and
  `runtime.rs` are touched again by #511–#513.

The sixth file the gate flagged, `packages/tddy-github/src/auth_service.rs`, was split in #510
itself (621 → 379 production lines, `auth_service/vault.rs` 290) with the developer's approval; so
was `packages/tddy-web/src/hooks/useAuth.ts` (520 → 413 lines, `authSession.ts` 124).

## What would close it

After `#keyring` lands, decompose each file with the `code-restructuring` skill (or by hand where
the engine refuses the seam) along the seams its record names, each in its own
behaviour-preserving `refactor(...)` commit with a green baseline before and after, and reconcile
each record at that change's wrap. `config.rs` and `runtime.rs` stay over budget after their first
seams; their records say what the rest takes. Delete this entry when all five are under budget, or
narrow it to the files that are not.

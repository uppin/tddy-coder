# 2026-09-24 — split `auth.rs` and `attach.rs` after the `#keyring` stack lands

**Category:** Deferred from `#keyring` 3/9 `store` (#510)
**Source:** `/pr-wrap` file-length gate on #510, range `origin/master..HEAD` (merge-base
`35cf2913`), production lines counted to the first `#[cfg(test)]`
**Consent:** the developer deferred both splits during #510's wrap, 2026-09-24

#510 grew two files that end over the 500-production-line budget. Neither is decomposed in #510.

| File | Production lines (`origin/master` → #510) | Record |
|---|---|---|
| `packages/tddy-daemon-auth/src/auth.rs` | 576 → 619 | `packages/tddy-daemon-auth/docs/code-issues/oversized-file-auth.md` |
| `packages/tddy-session-sync/src/attach.rs` | 517 → 520 | `packages/tddy-session-sync/docs/code-issues/oversized-file-attach.md` |

**Why after the stack.** Two stack parents (#508, #509) edited `auth.rs`; they are merged now, but
#511 still touches it, and a mechanical move inside a credential-store PR buries the security
review. `attach.rs` was already over budget on `origin/master`; #510 adds one struct field.

The third file the gate flagged, `packages/tddy-github/src/auth_service.rs`, was split in #510
itself (621 → 379 production lines, `auth_service/vault.rs` 290) with the developer's approval.

## What would close it

After `#keyring` lands, decompose both with the `code-restructuring` skill along the seams their
records name, each in its own behaviour-preserving `refactor(...)` commit with a green baseline
before and after, and delete both records.

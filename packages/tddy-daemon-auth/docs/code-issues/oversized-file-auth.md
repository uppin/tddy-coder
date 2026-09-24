# oversized-file: auth.rs — the daemon's auth service wiring

**Location:** `packages/tddy-daemon-auth/src/auth.rs`
**Category:** oversized-file
**Detected:** 2026-09-24 by the `/pr-wrap` file-length gate on #510 (`#keyring` 3/9)
**Metrics:** **622 production lines** (2026-09-24, #510 after its idle-vault follow-up, unchanged from its wrap; 576 on `origin/master` `35cf2913`), counted to the first `#[cfg(test)]` · budget 500 · ~1.24× over · 1,403 lines in all
**Thresholds breached:** length 622 > 500
**Restructure:** required — module seams (see below); `build_auth_entries_admitting` is recorded separately in `complexity-auth-build-auth-entries-admitting.md`
**Status:** Open. Split deferred with the developer's consent during #510's wrap, to a follow-up after the `#keyring` stack lands — `docs/dev/todo/2026-09-24-keyring-store-deferred-oversized-file-splits.md`

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-24 | 499 → 596 | #509 (`#keyring` 2/9) pushed it across the budget, as #509's wrap measured it (`docs/dev/todo/2026-09-24-keyring-desktop-login-grew-thirteen-over-budget-files.md`) |
| 2026-09-24 | 570 → 612 | #510 before its latest rebase, as its validation run measured it |
| 2026-09-24 | 576 → 619 | `origin/master` `35cf2913` (after #508 and #509 squash-merged) → #510 HEAD: `+43`, the credential-vault construction over `auth_storage` and the half-login rule extended to "cannot open" (`auth.rs:168-275`) |
| 2026-09-24 | 576 → 622 | #510 at wrap: `+3` more, the vaults built through `pending_logins::credential_vaults_in` so they carry `github.pending_login_ttl_seconds` (the lifetime's startup logs and the expiry sweep live in the new `pending_logins.rs`, not here) |
| 2026-09-24 | 622 → 622 | #510's post-wrap follow-up, net `0`: `pending_logins.rs` became `vault_lifetimes.rs` when its sweep took on idle open vaults; the `let ttl` line went, and one came back — `let lifetimes = VaultLifetimes::of(github)?`, which applies both lifetimes' defaults and ceilings and stops the daemon past one, before the `auth_storage` arm. The resolution, the startup logs and the sweep live in `vault_lifetimes.rs` (167 lines), not here |

## Why it grew, and why it is not split in #510

#508 and #509, the two stack parents that edited this file, are merged. #510 adds the construction
of `SessionVaults` over `auth_storage` and wires it into the auth service. Splitting the file inside
a security-sensitive feature PR would bury the reviewable diff, and #511 still touches it; the
developer deferred the split during #510's wrap.

## What would close it

Take the file under 500 with the `code-restructuring` skill, after `#keyring` lands. Candidate seams
(re-derive with `restructure anchors` against the tree of the day):

| Seam | Items |
|---|---|
| A | the provider/config resolution helpers `build_auth_entries_admitting` calls |
| B | the credential-vault construction and the `auth_storage` owner-only directory check |

Seam B alone is roughly the growth #510 added; together they leave the entry-point builders.

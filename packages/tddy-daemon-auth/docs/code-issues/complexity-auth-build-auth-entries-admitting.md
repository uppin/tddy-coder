# complexity: build_auth_entries_admitting

**Location:** `packages/tddy-daemon-auth/src/auth.rs:168` — `build_auth_entries_admitting`
**Moved:** 2026-09-24 — from `build_auth_entries_with` (`auth.rs:89`) by #509 (`#keyring` 2/9). The
body now lives in `build_auth_entries_admitting(config, web_host, web_port, &SessionTokens,
Option<Arc<dyn LoginAdmission>>)`; `build_auth_entries_with` is an 8-line wrapper passing `None` and
is under every threshold. Earlier: 2026-09-23, from `build_auth_entries` (`auth.rs:51`) to
`build_auth_entries_with` by #508 (`#keyring` 1/9); `build_auth_entries` is a 12-line wrapper.
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **110 lines** (2026-09-24, #510 after its idle-vault follow-up, unchanged from its wrap; 106 before #510, 92 before #509, 104 at detection) · **nesting depth 4** · 5 parameters (at the budget, not over) · 5 branch/match lines · 3 early exits
**Thresholds breached:** length 110 > 60 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — regressed 2026-09-24 (92 → 106 lines in #509, `#keyring` 2/9; 106 → 110 in #510, `#keyring` 3/9; 104 at detection) — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 104 | 4 | 6 | 3 | first detection, as `build_auth_entries` |
| 2026-09-23 | 92 | 4 | 6 | 3 | #508 (`#keyring` 1/9): the `livekit.api_secret` signer branches left, the body moved to `build_auth_entries_with`, and the `auth_storage` posture warning came in. Same structural scan re-run on `origin/master` (`77187dbe`) reproduces 104 / 6 / 3 for the old symbol |
| 2026-09-24 | 106 | 4 | 5 | 3 | 92 on the merge-base with `origin/master` (`4e7157d2`), as `build_auth_entries_with` → 106 after #509 (`#keyring` 2/9), as `build_auth_entries_admitting`: the body moved behind an `admission: Option<Arc<dyn LoginAdmission>>` fifth parameter (first-login enrolment for the desktop), threaded into every provider arm; the stub-vs-real `if` / `else if` chain became a match on `github_provider_kind` with separate `Confidential` and `Public` (device-flow) arms, each a rustfmt-wrapped `auth_service_entry` call. Branch lines 6 → 5 (the provider predicate left for `github_provider_kind`, 10 lines); nesting and exits unchanged. Hand structural scan, same method on base and HEAD. Split deferred with the developer's consent: `auth.rs` is touched by #510, #511 and #509's `## Boundaries` rules out splitting it in this stack (`docs/dev/todo/2026-09-24-keyring-desktop-login-grew-thirteen-over-budget-files.md`) |
| 2026-09-24 | 110 | 4 | 5 | 3 | 106 on `origin/master` (`35cf2913`) → 110 after #510 (`#keyring` 3/9): the `auth_storage` arm builds `SessionVaults` through `pending_logins::credential_vaults_in(dir, ttl)` — a `let ttl` and a rustfmt-wrapped call (+3) — where it built `FileGitHubTokenStore::new(dir)` and called its `probe_writable` method, now one free `probe_writable(dir)` call (−1); the arm's comment names the credential vault (+2). The three provider arms pass `credential_vaults` where they passed the token store, no length change. Branches, exits and nesting unchanged. Split deferred with the developer's consent to a follow-up after `#keyring` lands (`docs/dev/todo/2026-09-24-keyring-store-deferred-oversized-file-splits.md`) |
| 2026-09-24 | 110 | 4 | 5 | 4 | 110 at #510's wrap (`0918ff8c`) → 110 after its post-wrap follow-up: the `let ttl` line went and `let lifetimes = crate::vault_lifetimes::VaultLifetimes::of(github)?;` came in, before the `auth_storage` arm (net 0) — it resolves both vault lifetimes and returns the error that stops the daemon past a ceiling, so early exits 3 → 4 (one more `?`). Branches and nesting unchanged |

## What the tool found

The body is **92 lines**, 1.5x the 60-line ceiling at which `/analyze-clean-code` says a function
must be refactored.

The function carries **6 branch or match lines** and **3 early exits** (`return` / `?`). Its file
is 1,273 lines total, 499 of them production.

**How this was found.** `/jev-restructuring` ranked it 50 of 3,503 production units by
semantic shape (Jev classified it `unsure`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.

## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 106 > 60 — by `extract_method`
along the branch structure: the `auth_storage` block (token store, probe, posture warning) and the
provider selection (the `github_provider_kind` match: stub, confidential, public) are the two
contiguous runs. Anchor with
`tddy-tools restructure anchors`, never by hand, then prove the seam with `restructure check --deep`
against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.

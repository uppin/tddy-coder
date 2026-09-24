# complexity: build_auth_entries_with

**Location:** `packages/tddy-daemon-auth/src/auth.rs:89` — `build_auth_entries_with`
**Moved:** 2026-09-23 — from `build_auth_entries` (`auth.rs:51`) by #508 (`#keyring` 1/9). The body
now lives in `build_auth_entries_with(config, web_host, web_port, &SessionTokens)`;
`build_auth_entries` is a 12-line wrapper for a daemon with no signing identity and is under every
threshold.
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **92 lines** · **nesting depth 4** · 4 parameters · 6 branch/match lines · 3 early exits
**Thresholds breached:** length 92 > 60 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — partially fixed (104 → 92 lines; the length breach remains) — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 104 | 4 | 6 | 3 | first detection, as `build_auth_entries` |
| 2026-09-23 | 92 | 4 | 6 | 3 | #508 (`#keyring` 1/9): the `livekit.api_secret` signer branches left, the body moved to `build_auth_entries_with`, and the `auth_storage` posture warning came in. Same structural scan re-run on `origin/master` (`77187dbe`) reproduces 104 / 6 / 3 for the old symbol |

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

Bring it under the `/analyze-clean-code` thresholds — length 92 > 60 — by `extract_method`
along the branch structure: the `auth_storage` block (token store, probe, posture warning) and the
provider selection (stub vs real) are the two contiguous runs. Anchor with
`tddy-tools restructure anchors`, never by hand, then prove the seam with `restructure check --deep`
against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.

# complexity: build

**Location:** `packages/tddy-daemon/src/runtime.rs:623` — `build`
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **880 lines** (2026-09-24, #510 HEAD; 806 at detection) · **nesting depth 5** · 2 parameters · 20 branch/match lines · 15 early exits (the 2026-09-23 exit count; the first row's 10 used a different count)
**Thresholds breached:** length 880 > 60; nesting 5 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — regressed 2026-09-24 (806 → 879 lines since detection; +2 from #494, +45 from #508, +1 from #509, +1 from #510) — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 806 | 5 | 18 | 10 | first detection |
| 2026-09-22 | 833 | 5 | — | — | 831 on master before #494; +2 from #494 (`#carve` 8/11), the `SharedPresenterEventSink` cast at the `DaemonSessionHost::new` call. Nesting by indentation unchanged; branches and exits not re-derived |
| 2026-09-23 | 833 | 5 | — | — | touched by #520 (`#carve` 11/12) and **unchanged by it**, now at `runtime.rs:563`: `RpcHandlers::install(host)` added five lines and the four families' service and entry construction left for `RpcHandlers`, net zero. Nesting and `return`/`?` count identical to master; branches not re-derived |
| 2026-09-23 | 878 | 5 | 20 | 14 | 833 on `origin/master` (`4e260d7f`, after #520) → 878 after #508 (`#keyring` 1/9): the common-room peer registry resolved once before auth, the signing-key load, the `KeyDirectory` choice (`CommonRoomKeyDirectory` vs `StandaloneKeyDirectory`), `build_auth_entries_with`, and the advertised key handed to discovery. Same scan on master gives 18 branches / 13 exits, so #508 added 2 and 1; the first row's 10 exits used a different exit count. Nesting by indentation identical on master and HEAD. Split deferred to a follow-up after `#keyring` lands, as for `oversized-file-runtime` |
| 2026-09-24 | 879 | 5 | 20 | 15 | 878 on the merge-base with `origin/master` (`4e7157d2`) → 879 after #509 (`#keyring` 2/9): `build_auth_entries_with` became `build_auth_entries_admitting`, taking `first_login_enrolment(&config, &options)?` as a fifth argument (+1 line, +1 `?`). The enrolment decision itself went into a new free function above `build`, not into it. Branches unchanged; nesting by indentation identical. Split deferred with the developer's consent to a follow-up after `#keyring` lands, as for `oversized-file-runtime` |
| 2026-09-24 | 880 | 5 | 20 | 15 | 879 on `origin/master` (`35cf2913`) → 880 after #510 (`#keyring` 3/9): the `if let Some(store) = auth_result.github_token_store` injection becomes `if let Some(vaults) = auth_result.credential_vaults` and gains one line, `tddy_daemon_auth::pending_logins::spawn_pending_login_sweep(&vaults)` — the pending-login expiry sweep. The vaults' construction and the sweep's body went into `tddy-daemon-auth`'s `pending_logins.rs`, not here. Branches, exits and nesting unchanged (the `if let` was already there; no `?` added). Split deferred with the developer's consent to a follow-up after `#keyring` lands (`docs/dev/todo/2026-09-24-keyring-store-deferred-oversized-file-splits.md`) |

## What the tool found

The body is **806 lines**, 13.4x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored.

The function carries **18 branch or match lines** and **10 early exits**
(`return` / `?`). Its file is 1481 lines total, 1421 of them production, across 14 functions.

**How this was found.** `/jev-restructuring` ranked it 86 of 3,503 production units by
semantic shape (Jev classified it `god_function`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.


## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 806 > 60; nesting 5 > 4 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.

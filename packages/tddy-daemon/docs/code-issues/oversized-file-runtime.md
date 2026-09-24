# oversized-file: runtime.rs — the daemon's composition root

**Location:** `packages/tddy-daemon/src/runtime.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 by the `/pr-wrap` file-length gate, independently on #498 and #518
**Metrics:** **1,620 production lines** (2026-09-24, #510 HEAD; 1,619 before #510, 1,562 before #509, 1,513 before #508) · budget 500 · **~3.2× over** · residue function `build` is **880 lines** (879 before #510, 878 before #509)
**Thresholds breached:** length 1620 > 500; `build` 880 > 60
**Restructure:** required — three `extract_module --to_file` seams **plus** function splitting
**Status:** Open — regressed 2026-09-24 (1,562 → 1,619 in #509, `#keyring` 2/9; 1,619 → 1,620 in #510, `#keyring` 3/9). Pre-existing; #498, #518 and #494 grew it and deferred with explicit developer consent; #508, #509 and #510 grew it and deferred the split because dependents #510–#513 touch this file

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-19 | 1,420 | baseline, before either PR |
| 2026-09-19 | 1,423 | after #498 — three lines |
| 2026-09-19 | 1,479 | after #518 — 59 lines |
| 2026-09-19 | 1482 | both merged |
| 2026-09-22 | 1,519 | master before #494 |
| 2026-09-22 | 1,521 | after #494 (`#carve` 8/11) — two lines |
| 2026-09-23 | 1,513 | after #520 (`#carve` 11/12) — −8: `BinaryLocalSocketServices` names the four handler types in fewer lines, and the families' construction moved to `tddy-daemon-rpc`'s `RpcHandlers`; `build` itself unchanged at 833 |
| 2026-09-23 | 1,562 | master 1,513 → 1,562 after #508 (`#keyring` 1/9: the signing identity and key directory in `build`) — grown by #508; split deferred to a follow-up after #keyring lands because dependents #509–#513 touch it |
| 2026-09-24 | 1,619 | 1,562 on the merge-base with `origin/master` (`4e7157d2`) → 1,619 after #509 (`#keyring` 2/9): `first_login_enrolment` (first-login admission for an embedded desktop, and the refusal of an embedding host that names no config file) and `this_process_os_user` (+56, above `build`), plus `build_auth_entries_admitting` taking the admission (+1 inside `build`). Grown; the split is deferred with the developer's consent to a follow-up after `#keyring` lands, because #510, #511 and #512 touch this file (`docs/dev/todo/2026-09-24-keyring-desktop-login-grew-thirteen-over-budget-files.md`) |
| 2026-09-24 | 1,620 | 1,619 on `origin/master` `35cf2913` → 1,620 after #510 (`#keyring` 3/9): inside `build`, the injection of `auth_result.github_token_store` becomes `credential_vaults` and gains one line, `tddy_daemon_auth::pending_logins::spawn_pending_login_sweep(&vaults)` — the pending-login expiry sweep, spawned where the vaults are handed to the connection host. `build` 879 → 880. Grown; the split is deferred with the developer's consent to a follow-up after `#keyring` lands, because #511–#513 touch this file (`docs/dev/todo/2026-09-24-keyring-store-deferred-oversized-file-splits.md`) |
| 2026-09-24 | 1,620 | touched, unchanged: #510's post-wrap follow-up renames the sweep it spawns to `tddy_daemon_auth::vault_lifetimes::spawn_credential_sweep(&vaults)` — the same one line, which now also closes an open vault nothing has used for `github.open_vault_idle_ttl_seconds`. The idle eviction and the sweep's second kind live in `tddy-credentials` and `tddy-daemon-auth`, not here. `build` still 880 |

## What the gate found

Already 2.8× the budget before either PR touched it.

**#498's contribution is three lines**: eight `crate::relay_idle` / `crate::user_sessions_path`
paths re-pointed at `tddy_session_lifecycle` when the re-export shims were deleted, which reflowed
three lines under `rustfmt`.

**#518's is 59**: `DaemonChildren`, the `session_host` field, and the shutdown wiring that makes
SIGTERM reach the workspace-jail registry.

**#494's is two lines**: the Telegram hooks are injected into the connection service as a
`SharedPresenterEventSink` (a `.map(|hooks| hooks as …)` cast that `rustfmt` wraps over three
lines). Its other edits re-point `tddy_session_lifecycle::telegram_*` paths at
`tddy_telegram_control` and rewrite one comment, with no net change in length. Deferred with the
developer's consent at `/pr-wrap` — `docs/dev/todo/2026-09-22-telegram-control-plane-left-over-budget-by-a-move-only-node.md`.

## What would close it — designed seams

| Seam | Items | Out |
|---|---|---|
| A | `DaemonChildren` … `impl RuntimeTasks` | ~265 |
| B | `apply_env_overrides`, `env_var`, `data_dir_from`, `tddy_data_dir_for` | ~83 |
| C | `TelegramWiring`, `build_telegram` | ~114 |

All three → parent ~1017. **The module seams alone cannot close this**: the residue is `build`,
**819 lines**, one function.

## The residue, and why it is the easy case

`build` is a **free function**, not an impl member. So `extract_method --variant module` puts the
extracted helper at module level where `anchors` *can* address it, and `extract_module_to_file`
then moves it out — no impl in the way, unlike `svc_start_session_core.rs`.

Cut along the wiring groups inside `build`; each subsystem's construction is a contiguous run.
Three or four cuts of ~200 lines each take the file under 500 **and** the function under the
60-line ceiling.

⚠ Line numbers must be re-derived with `restructure anchors` — the seams above were located
against the 1,479-line tree, before this merge.

# oversized-file: runtime.rs — the daemon's composition root

**Location:** `packages/tddy-daemon/src/runtime.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 by the `/pr-wrap` file-length gate, independently on #498 and #518
**Metrics:** **1,562 production lines** (2026-09-23; 1,513 before #508) · budget 500 · **~3× over** · residue function `build` is **833 lines**
**Thresholds breached:** length 1562 > 500; `build` 833 > 60
**Restructure:** required — three `extract_module --to_file` seams **plus** function splitting
**Status:** Open — pre-existing; #498, #518 and #494 grew it and deferred with explicit developer consent

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

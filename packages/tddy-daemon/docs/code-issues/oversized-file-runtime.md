# oversized-file: runtime.rs

**Location:** `packages/tddy-daemon/src/runtime.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 — `/pr-wrap` step 3.5 file-length gate
**Metrics:** **1,479 production lines** · budget 500 · **3.0× over** · residue function `build` is **819 lines**
**Thresholds breached:** length 1479 > 500; `build` 819 > 60
**Restructure:** three `extract_module --to_file` seams **plus** function splitting — designed, not applied
**Status:** Open — **unclaimed**

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-19 | 1479 | 1420 → 1479 in this PR (`DaemonChildren`, the `session_host` field, the shutdown wiring) |

## What would close it — designed seams

| Seam | Items | Out |
|---|---|---|
| A | `DaemonChildren` … `impl RuntimeTasks` (L199–463) | ~265 |
| B | `apply_env_overrides`, `env_var`, `data_dir_from`, `tddy_data_dir_for` (L464–546) | ~83 |
| C | `TelegramWiring`, `build_telegram` (L1366–1479) | ~114 |

All three → parent **~1017**. **The module seams alone cannot close this**: the residue is
`build`, L547–1365, **819 lines**, one function.

## The residue, and why it is the easy case

`build` is a **free function**, not an impl member. So `extract_method --variant module` puts the
extracted helper at module level where `anchors` *can* address it, and `extract_module_to_file` then
moves it out — no impl in the way, unlike `svc_start_session_core.rs`.

Cut along the wiring groups inside `build`; each subsystem's construction is a contiguous run.
Three or four cuts of ~200 lines each take the file under 500 **and** the function under the
60-line ceiling.

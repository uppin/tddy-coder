# oversized-file: run.rs

**Location:** `packages/tddy-coder/src/run.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 — `/pr-wrap` step 3.5 file-length gate
**Metrics:** **2,682 production lines** (5,083 total) · budget 500 · **5.4× over** · residue `run_daemon` is **620 lines**
**Thresholds breached:** length 2682 > 500; `run_daemon` 620 > 60; `run_with_args` ~196 > 60; `run_main` ~153 > 60
**Restructure:** five `extract_module --to_file` seams **plus** function splitting — designed, not applied
**Status:** Open — **unclaimed**

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-19 | 2682 | 2679 → 2682 in this PR — a **three-line** change (`ClientConfig` gains one field) |

## What would close it — designed seams

| Seam | Items | Out |
|---|---|---|
| A | `Args`, `CoderArgs`, `DemoArgs`, `is_debug_mode`, `effective_log_config`, `parse_log_level`, the two `impl From<…> for Args` (L438–974) | ~537 |
| B | `validate_livekit_args` … `run_codex_oauth_login` (L1090–1247) | ~158 |
| C | `print_session_info_on_exit` … `write_post_tui_workflow_exit` (L2314–2425) | ~112 |
| D | `resolve_cursor_agent_binary` … `BUILTIN_BACKEND_AGENT_IDS` (L2509–2682) | ~174 |
| E | `recipe_arc_for_args` … `resolve_agent_repo_root` (L30–284) | ~255 |

Five seams → parent **~1446**. Residue: `run_daemon` L1694–2313 (620), plus `run_with_args` (~196)
and `run_main` (~153).

## The residue

`run_daemon` is a **free function**, so the same clean route as `runtime.rs::build`:
`extract_method --variant module` → `extract_module_to_file`.

## Constraints a later session must know

This file has **three** `#[cfg(test)]` modules (L2683, L2880, L4359). Seam A in particular needs
`reexport: "glob"`, or a `named` facade covering everything all three reach.

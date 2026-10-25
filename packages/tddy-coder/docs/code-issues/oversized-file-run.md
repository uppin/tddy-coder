# oversized-file: run.rs

**Location:** `packages/tddy-coder/src/run.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 — `/pr-wrap` step 3.5 file-length gate
**Metrics:** **2,707 production lines** (2026-09-24; 2,682 at detection, 5,083 total then) · budget 500 · **5.4× over** · residue `run_daemon` is **620 lines** (not re-measured 2026-09-24)
**Thresholds breached:** length 2682 > 500; `run_daemon` 620 > 60; `run_with_args` ~196 > 60; `run_main` ~153 > 60
**Restructure:** five `extract_module --to_file` seams **plus** function splitting — designed, not applied
**Status:** Open — regressed 2026-09-24 (2,682 → 2,707 in #509, `#keyring` 2/9; split deferred with consent) — **unclaimed**

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-19 | 2682 | 2679 → 2682 in this PR — a **three-line** change (`ClientConfig` gains one field) |
| 2026-09-22 | 2682 | 2682 → 2682 in #493 — catalog paths repointed to `tddy_session_catalog`; not grown |
| 2026-09-24 | 2707 | 2682 on the merge-base with `origin/master` (`4e7157d2`) → 2707 after #509 (`#keyring` 2/9): the `standalone_auth_provider` / `StandaloneAuthProvider` auth-flow declaration (step 1 V9) and transport stamping. Grown; the split is deferred with the developer's consent to a follow-up after `#keyring` lands, because #510 and #511 touch this file (`docs/dev/todo/2026-09-24-keyring-desktop-login-grew-thirteen-over-budget-files.md`). The step 3.5 gate read 2715; step 4's `build_auth_service_entry` cleanup took 8 back. First `#[cfg(test)]` now at L2708 |
| 2026-10-21 | 2655 | 2655 → 2655 in #690 (`#satellite` 15/15): one string in `verify_tddy_tools_available` repointed to `cargo build -p tddy-tools-cli`. Touched, not grown |

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

## Measurement update 2026-10-15 — #622 (local gh)

**Regressed, split deferred with consent:** 2,673 → 2,678 production lines. `ToolHostHandlers::from_flags` chains `.with_github_source(TDDY_GITHUB_SOURCE)`. See [2026-10-15-local-gh-oversized-files-deferred.md](../../../../docs/dev/todo/2026-10-15-local-gh-oversized-files-deferred.md).

## Measurement update 2026-10-19 — #satellite 8/13 (#630)

**Shrank, split still deferred:** 2,678 → 2,655 production lines (-23). The coder's room participant now goes through the `SessionParticipantHost` port (`participant_host.rs`): the four per-credential `livekit` join branches and their token-provider plumbing collapse into `participant_join_from_cli`. Dependent PRs (#631+, `feature/satellite/status-ui` and siblings) also touch this file, so no decomposition here. Still unclaimed.

## Measurement update 2026-10-22 — #provider-route 4/9 (#665)

**Narrowed, still over:** 2,655 → 2,595 production lines. `--provider-route-path` added 34 (to 2,689); the wrap moved route loading and application to `provider_route_arg.rs` and the Codex binary resolution (`resolve_codex_binary`, `resolve_codex_acp_binary`, `codex_acp_beside_resolved_codex`, `resolve_executable_on_path` — part of seam D) to `codex_binaries.rs`. Still unclaimed.

## Measurement update 2026-10-22 — #provider-route 7/9 (#668)

**Regressed, split deferred:** 2,598 → 2,620 production lines (2,598 is this count on the rebased base). `--opencode-cli-path` / `TDDY_OPENCODE_CLI` (`CoderArgs`, `Args`, the `From` impls), the `opencode` arm of `create_backend` and its path parameter, the factory's path clone, and `"opencode"` in `BUILTIN_BACKEND_AGENT_IDS`. The file is stack-shared — `#provider-route` 8/9 (#669) and 9/9 (#670) sit on this branch — so no decomposition here. Still unclaimed.

## Measurement update 2026-10-25 — #start-parity 6/9 (#755)

**Regressed, split deferred:** 2,620 → 2,626 production lines. `run_daemon` starts
`session_participant::pending_question::spawn_pending_question_recorder` beside the ACP transcript
writer (the recorder itself is the new `session_participant/pending_question.rs`, 76). Already over
budget before this PR, which did not push it past; recorded, not split, by developer instruction.

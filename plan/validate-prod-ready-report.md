# Production Readiness Report

## Summary

The feature correctly plumbs `workflow_session_id` through `PresenterState` and `presenter_impl`, drives the status bar segment from `ProgressEvent::SessionStarted` and `start_workflow`, and clears it on terminal workflow outcomes. The TUI draws the spinner and segment without `println!`/`eprintln!`. **Main production concern:** several `log::debug!` calls sit on the **per-frame** status-bar path (`render::status_bar_text_for_draw` and `ui::first_hyphen_segment_of_workflow_session_id` / `prepend_activity_to_status_line`). With debug logging enabled for these modules, that produces high-volume logs and extra work every frame—atypical for “debug” semantics and risky for performance or log noise. Secondary: full `workflow_session_id` appears in some debug log lines (presenter + draw path); UUID-shaped ids are usually not secrets but are correlation identifiers—avoid shipping verbose session details to shared logs if policy requires minimization.

## Checklist Findings

| Area | Finding |
|------|---------|
| **Error handling** | No new error surfaces in the four files for this feature. `Presenter` continues to use existing patterns (`WorkflowComplete(Err)` → `log::error!`, activity log, `ErrorRecovery`). Session segment parsing does not fail loudly—it falls back to the em dash placeholder (documented behavior per `plan/evaluation-report.md`). |
| **Logging (`log::`)** | **info** in `presenter_impl` for `SessionStarted` is high-level (“TUI status segment will use id prefix”)—appropriate. **debug** lines that print **full** `session_id` in `poll_workflow` and `start_workflow` are reasonable for troubleshooting. **Problem:** **debug** in `ui.rs` and `render.rs` runs on **every draw** when building the status line (spinner tick, segment extraction, prepend). That can flood logs and adds string work (`prepend_activity_to_status_line` builds a 24-char prefix for log messages) on the hot path. Prefer removing or gating per-frame debug, or sampling once per session id change. |
| **Configuration** | No new env flags or config structs in these files. Behavior is driven by presenter events and existing `RUST_LOG` / logging setup. |
| **Security (secrets in logs)** | No passwords or API keys introduced. **Session / engine identifiers** are logged at **debug** in presenter and (if enabled) on the draw path with full `workflow_session_id` in `render.rs`—treat as sensitive in strict environments; consider truncating or hashing in logs if required by policy. |
| **Performance** | **Per frame:** `status_bar_text_for_draw` allocates a full status `String` (expected for ratatui). `first_hyphen_segment_of_workflow_session_id` allocates `Cow::Owned` + lowercase copy only for valid 8-hex UUID first fields (small, bounded). **Avoidable cost:** debug logging and `prepend_activity_to_status_line`’s debug-only `tail.chars().take(24).collect::<String>()` when debug is on. Activity log `join("\n")` in `draw` is pre-existing, not introduced by this feature. |
| **TUI constraints** | **No `println!` / `eprintln!`** under `packages/tddy-tui/src` (verified). Draw path uses `log::debug!` only—does not corrupt the terminal **if** default log output is not attached to the TUI tty (typical: stderr elsewhere or disabled). Confirm deployment does not route `log` to the same console as ratatui at debug levels. |

## Risks

1. **Log storm / frame cost** — Per-frame `log::debug!` in status bar construction can dominate I/O and CPU when `RUST_LOG` includes `tddy_tui` or `tddy_core::...` at debug in production-like debugging sessions.
2. **Stale session segment** — `restart_workflow` does not reset `workflow_session_id`; it relies on prior clears (`WorkflowComplete`) or later `SessionStarted` / `start_workflow`. Edge cases with rapid restarts could briefly show an old segment until the next event (low likelihood if completion paths always clear).
3. **Non-UUID engine ids** — `first_hyphen_segment_of_workflow_session_id` only shows 8 hex chars; other formats always show the em dash (product/UX, noted in evaluation report).

## Recommendations

1. **Remove or relocate** per-frame `log::debug!` from `render::status_bar_text_for_draw`, `ui::first_hyphen_segment_of_workflow_session_id`, and `prepend_activity_to_status_line` (or guard with a `static` last-logged session + tick modulo to rate-limit). Keep optional **trace** if deep diagnosis is needed, with explicit warning in comments.
2. **Truncate** session ids in any remaining debug logs (e.g. first 8 chars + `…`) if log aggregation must minimize identifiers.
3. **Document** for operators: avoid `RUST_LOG=debug` for `tddy_tui` on the same tty as the interactive TUI if the logging backend writes to stderr that overlaps the alternate screen (project rules already discourage corrupting the ratatui display—logging is the main residual risk).

## Conclusion

The state machine and UI wiring for the status spinner and session segment are **fit for production** from a correctness and TUI-safety standpoint. **Before treating as fully production-hardened**, tighten **logging on the draw path**: per-frame debug logs are the top issue for operational behavior (noise, cost, and identifier exposure). Addressing that yields a cleaner production profile without changing user-visible behavior.

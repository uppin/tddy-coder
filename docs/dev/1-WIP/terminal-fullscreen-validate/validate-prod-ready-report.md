# Validate: production readiness (TUI / presenter worktree changeset)

Scope: `packages/tddy-core` presenter worktree + state, and `packages/tddy-tui` ratatui integration (heartbeat, Virtual TUI, markdown plan tail, status bar). This report supersedes the prior web-focused draft at this path for the current subagent task.

## Executive summary

The changeset improves status UX (spinner vs idle heartbeat, worktree segment, markdown plan tail, Virtual TUI cadence) and is generally defensive about I/O errors. **One architectural gap blocks the worktree status segment in the live app:** `PresenterState::active_worktree_display` is updated only inside the presenter’s `poll_workflow`, while the interactive TUI keeps a **clone** of state synchronized exclusively via `PresenterEvent`. No event carries `active_worktree_display`, and `apply_event` does not set it—so the status-bar injection path in `render.rs` will not see worktree switches after attach. Secondary concerns: **hot-path logging at `info`** (`event_loop`, `markdown_plan_action_layout_for_view`), **CSI stripping heuristics** for cursor-only frame detection, **`ratatui` unstable feature flag** coupling, and **repo hygiene** for `.red-tddy-test-output.txt`. Markdown viewer pays **two wrapped line-count passes** (and `Text` clones) per frame while in `MarkdownViewer`, which can matter for very large PRDs.

## Checklist

| Area | Verdict | Summary |
|------|---------|---------|
| Error handling / panics (TUI paths) | **Pass** | `event::poll` uses `unwrap_or(false)`; Virtual TUI handles terminal creation failure; resize/draw errors logged without panic. `frame_buf.lock().unwrap()` can panic only on mutex poison (abnormal). |
| `active_worktree_display` vs broadcast sync | **Fail** | Field updated in presenter only; TUI `apply_event` never copies it—status worktree segment ineffective for `run_event_loop` + `connect_view` architecture. |
| Logging levels / hot paths | **Concern** | `log::info!` on every frame when editing caret active (`event_loop.rs`); `log::info!` every `MarkdownViewer` draw via `markdown_plan_action_layout_for_view`; `worktree_display` uses `info` with path data. |
| `ratatui` `unstable-rendered-line-info` | **Concern** | Required for `Paragraph::line_count`; ties release stability to ratatui unstable API—plan upgrades carefully and document the dependency. |
| Security / paths / secrets | **Concern** | Activity log and logs emit full `path.display()` on worktree switch; formatted segment is basename/truncated (good for UI). No tokens in reviewed paths. |
| Performance: heartbeat / Virtual TUI throttle | **Pass** | Idle clarification ~1 Hz periodic render; Running ~200 ms; cursor-only frames throttled (80 ms). |
| Performance: markdown `line_count` | **Concern** | Up to two `markdown_paragraph_wrapped_line_count` calls + `text_md.clone()` per frame in `MarkdownViewer`. |
| CSI heuristic (`strip_cursor_csi_sequences`) | **Concern** | Byte scan for `ESC [` … final byte; strips CUP (`H`/`f`) and `?25h`/`?25l`. Risk of misclassification (cursor-only vs content) if sequences diverge in edge terminals; incomplete vs full CSI taxonomy. |
| `.red-tddy-test-output.txt` hygiene | **Concern** | Untracked artifact at repo root; add to `.gitignore` or remove—avoid accidental commit. |

## Detailed findings

### 1. Worktree display not replicated to the view (`Fail`)

- **Presenter:** `WorkflowEvent::WorktreeSwitched` sets `state.active_worktree_display` from `format_worktree_for_status_bar` and broadcasts `PresenterEvent::ActivityLogged` with `Worktree: {}` (`presenter_impl.rs`).
- **TUI:** `inject_worktree_into_status_line` reads `state.active_worktree_display` (`render.rs`).
- **Gap:** `apply_event` in `virtual_tui.rs` handles `ActivityLogged` by appending to `activity_log` only; it does **not** set `active_worktree_display`. `PresenterEvent` has no dedicated variant, and `CriticalPresenterState` does not include this field—so lag recovery cannot restore it either.
- **Integration:** `tddy-coder` runs the presenter in a worker thread and the TUI with `connect_view()`’s snapshot + broadcast (`run.rs`). The TUI never reads the presenter mutex for render state—so the new status segment will remain absent unless an event or shared snapshot is added.

### 2. Error handling and panic risk (`Pass` with notes)

- `run_event_loop`: `Terminal::new`, `draw`, and terminal teardown use `?` or `let _ =` appropriately; panic hook restores terminal on unwind.
- `virtual_tui`: `Terminal::with_options` failure returns early with `log::error!`.
- `strip_cursor_csi_sequences` / parsers: bounded scans; incomplete sequences fall through without panic.
- **Note:** `frame_buf.lock().unwrap()` in `render_and_send` will panic if the mutex is poisoned after a panicking writer callback—unlikely in production if `draw` does not panic.

### 3. Logging (`Concern`)

- **`event_loop.rs` (~20 Hz draw loop):** `log::info!("local_tui: editing caret active — crossterm Show")` runs whenever `editing_prompt_cursor_position` is `Some`—i.e. every frame while the user edits a prompt. Should be `trace` or `debug`, or logged only on transition.
- **`render.rs`:** `markdown_plan_action_layout_for_view` logs at **`info`** including `markdown_at_end` on **every** `MarkdownViewer` draw—high volume during plan review.
- **`worktree_display.rs`:** `log::info!` for “using” / “truncated” / empty path includes `Debug` of paths or labels—acceptable for rare events but “empty path” logs full `Path` at info; prefer `debug` for routine formatting outcomes.
- **`presenter_impl.rs`:** `log::info!` on `WorktreeSwitched` when display is set—low frequency; OK relative to event loop noise.

### 4. `ratatui` unstable feature (`Concern`)

- `packages/tddy-tui/Cargo.toml`: `features = ["unstable-rendered-line-info"]` enables `Paragraph::line_count` used by `markdown_paragraph_wrapped_line_count`.
- **Risk:** Semver-stable ratatui may still change or gate unstable APIs—CI and release notes should treat this as a conscious dependency on unstable surface area.

### 5. Security and path handling (`Concern`)

- **UI:** `format_worktree_for_status_bar` avoids dumping full filesystem paths in the status string; truncation on UTF-8 char boundaries is sound.
- **Activity log:** `Worktree: {}` uses `path.display()`—full path visible in-scroll and in any log aggregation of activity text—may be sensitive in shared logs or screenshots.
- **Secrets:** No API keys or tokens observed in the reviewed files.

### 6. Performance (`Pass` / `Concern`)

- **Heartbeat / Virtual TUI:** `virtual_tui_periodic_render_interval` uses 1 s in clarification wait and 200 ms otherwise; `virtual_tui_cursor_only_frame_min_interval` is 80 ms—aligned with PRD-style throttling.
- **Markdown viewer:** Each frame may call `markdown_paragraph_wrapped_line_count` twice (body-only then body + tail) and clone `Text` for the extended document—O(document) work per frame. For typical PRDs this is fine; for multi-megabyte markdown, consider caching line counts keyed by `(content_hash, width)` or debouncing scroll-only updates (larger change).

### 7. CSI heuristic (`Concern`)

- **`virtual_tui.rs` `strip_cursor_csi_sequences`:** Removes sequences ending with `H`/`f` or containing `?25h` / `?25l`. Legitimate content-changing sequences could theoretically overlap rare patterns; more likely, **false “cursor-only”** classification could suppress needed frames or **false “content”** could emit extra traffic—monitor with real clients (Ghostty, xterm, web PTY).
- Truncated CSI at buffer end copies a single byte and advances—safe but may desync until more bytes arrive (normal for streaming PTY).

### 8. Repository hygiene (`Concern`)

- **`.red-tddy-test-output.txt`:** Appears as an untracked local artifact (agent/test output). `.gitignore` already lists `.verify-result.txt` but not this file—recommend aligning ignore rules or deleting the file after use to avoid accidental commits.

## Recommendations

1. **Fix worktree status sync (blocking):** Add a `PresenterEvent` (e.g. `ActiveWorktreeDisplayChanged(Option<String>)`) or extend `CriticalPresenterState` + broadcast whenever `active_worktree_display` changes, and handle it in `apply_event`. Alternatively, derive the display string idempotently from the last `ActivityLogged` worktree entry (fragile—prefer explicit event).
2. **Demote hot-path logs:** Change `event_loop` caret message and `markdown_plan_action_layout_for_view` to `trace`/`debug`. Keep `info` for rare lifecycle events.
3. **Ratatui:** Document the unstable feature in package changelog or `AGENTS.md` toolchain notes; pin ratatui version deliberately and run tests on upgrade.
4. **Privacy:** Use `log::debug` for full paths in worktree formatting; keep user-visible activity line as product requires.
5. **Markdown perf (optional):** If large PRDs are expected, cache wrapped line counts when `content` and `width` are unchanged between frames.
6. **Hygiene:** Add `.red-tddy-test-output.txt` to root `.gitignore` (or remove file) consistent with `.verify-result.txt`.

---

**Path written:** `docs/dev/1-WIP/terminal-fullscreen-validate/validate-prod-ready-report.md`

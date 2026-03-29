# Clean-code analysis: TUI PRD touchpoints

**Scope:** `packages/tddy-core/src/presenter/worktree_display.rs`, `packages/tddy-tui/src/render.rs`, `status_bar_activity.rs`, `event_loop.rs`, `virtual_tui.rs`  
**Focus:** Naming, complexity, duplication (cursor / CSI vs ratatui), cohesion / `render.rs` size, docs, `MarkdownPlanActionLayout`, ratatui API stability.

---

## Summary

The PRD-facing pieces are **test-backed and mostly readable**, with clear module-level intent in `worktree_display.rs` and `status_bar_activity.rs`. The main structural risk is **`render.rs` as a large, multi-responsibility module** (~1.4k lines including tests) that owns layout math, prompt strings, hardware cursor placement, status-bar text, and several mode-specific widgets. **`virtual_tui.rs` correctly separates streaming concerns** (frame diff, CSI stripping, input parsing) but remains long and overlaps behavior with `event_loop.rs` for mouse/select intents.

**Explicit ratatui instability:** `tddy-tui` enables Cargo feature `unstable-rendered-line-info` and uses `Paragraph::line_count` in `markdown_paragraph_wrapped_line_count` (`render.rs`). That couples markdown scroll/end detection to a **non-stable** ratatui API surface.

**`MarkdownPlanActionLayout`:** Production code always returns `TailWithDocumentWhenAtEnd`; `FixedFooterAlways` is **not constructed** in app code—only kept for a **negative test** assertion and marked `#[allow(dead_code)]`.

---

## Strengths

- **`worktree_display.rs`:** Small, single-purpose module; UTF-8-safe truncation; module and public API documented; focused unit test tied to PRD acceptance.
- **`status_bar_activity.rs`:** Strong cohesion—agent vs user-wait, frozen elapsed, idle heartbeat, Virtual TUI cadence in one place; Unicode glyph choices documented in the module header; tests map cleanly to PRD phrases.
- **`event_loop.rs`:** Straightforward control flow; explicit policy helper `local_tui_editing_cursor_policy`; panic hook restores terminal; editing caret visibility integrated with `editing_prompt_cursor_position` after `draw`.
- **`virtual_tui.rs`:** Well-commented streaming model (diff, cursor-only throttle, clear-on-first-frame); `drain_presenter_broadcast` documents `Lagged` semantics and critical-state resync; solid unit tests for parsers and broadcast edge cases.
- **Shared `draw` path:** Local and Virtual TUI both call `render::draw`, which keeps behavior aligned (validated by tests in `render.rs`).

---

## Issues (by severity)

### High

1. **`render.rs` size and mixed responsibilities (SRP).** One file owns: wrapped line counting, markdown tail actions, prompt text matrix, terminal cursor geometry, status bar assembly, main `draw`, and four sizeable render helpers (`render_question`, `render_document_review`, `render_error_recovery`, `render_inbox`) plus a large `tests` module. This raises merge conflict risk and makes PRD changes harder to review in isolation.

2. **Dependency on unstable ratatui API.** `packages/tddy-tui/Cargo.toml` sets `ratatui` with `features = ["unstable-rendered-line-info"]`. `markdown_paragraph_wrapped_line_count` uses `Paragraph::new(text.clone()).wrap(...).line_count(width)` (`render.rs`). Upstream breaking changes or feature removal would break scroll/end-of-doc logic without a compile-time “stable” alternative in-tree.

### Medium

3. **Duplication: menu row styling.** The pattern `"> "` / `"  "` plus `Modifier::REVERSED` vs default style repeats across `markdown_plan_action_tail_lines`, `render_question` (Select and MultiSelect branches), `render_document_review`, `render_error_recovery`, and `render_inbox` (`render.rs`). Same UX primitive reimplemented many times.

4. **Duplication: prompt string vs cursor position.** `prompt_text` and `editing_prompt_cursor_position` both match on `AppMode` and rebuild similar strings with `PFX` and empty/non-empty branches (`render.rs`). Drift between visible text and byte cursor index is a recurring bug class.

5. **Manual char-wrapped prompt vs ratatui.** `draw` splits `prompt_text_str` into `Line`s by fixed-width char chunks to match `prompt_height` (`render.rs`); `terminal_position_for_byte_cursor_in_char_wrapped_prompt` mirrors that layout. This is intentional but **must stay aligned** with `prompt_height` in `layout`—no single source of truth in ratatui for “where is the caret.”

6. **`markdown_plan_action_layout_for_view` is mostly ceremonial.** It takes `markdown_at_end` but always returns `TailWithDocumentWhenAtEnd`; the result is assigned to `_plan_layout` and unused in `draw` (`render.rs`). The function and enum encode historical layout strategy more than current behavior.

7. **`MarkdownPlanActionLayout::FixedFooterAlways` is a test-only phantom variant.** It is never constructed in production; tests use `assert_ne!(..., FixedFooterAlways)` to assert the retired mode. The variant is `#[allow(dead_code)]` with a comment (“layout tests”) (`render.rs`). This is honest but awkward API surface.

8. **`worktree_display.rs` logging at `info!` on normal paths** (non-empty display, truncation) may be noisy in production compared to `trace`/`debug` used elsewhere in the TUI stack.

### Low

9. **Parallel event-loop behaviors.** `event_loop.rs` and `process_virtual_tui_input_chunk` (`virtual_tui.rs`) both: drain broadcast (loop vs single path), handle keys, send `SelectHighlightChanged` on Up/Down in Select, wire mouse to `handle_mouse_event`. Divergence risk on future key/mouse policy changes.

10. **`strip_cursor_csi_sequences` heuristic** (`virtual_tui.rs`): byte-scans ESC `[` sequences; `?25h` / `?25l` detection uses `windows(4)` on the sequence slice. Works for crossterm/ratatui output today but is **not a full CSI parser**—unusual sequences could be miscategorized (documented risk for maintainers).

11. **Minor doc clutter in `render.rs`:** Two adjacent `///` blocks before `SPINNER_FRAMES` (“Draw the TUI…” appears twice around the const), which is slightly confusing when skimming.

---

## Refactor suggestions (prioritized)

1. **Split `render.rs` by responsibility** (highest leverage): e.g. `markdown_viewer_render.rs` (scroll, tail actions, line counting), `prompt_render.rs` (prompt text + cursor helpers), `widgets_menus.rs` (question, document review, error recovery, inbox), keep `draw` orchestration in a thin `render.rs` or `frame.rs`. Preserve public `draw` and re-export if needed to avoid churn in `event_loop` / `virtual_tui`.

2. **Track / eliminate unstable ratatui usage:** Prefer a stable way to compute wrapped line counts (custom wrap pass, or a pinned abstraction) so `unstable-rendered-line-info` can be dropped from `Cargo.toml` when ratatui stabilizes or changes the feature. Until then, **document the pin** in module-level comments next to `markdown_paragraph_wrapped_line_count`.

3. **Extract a small helper for “selected row line”** (prefix + optional reversed style) and reuse in Approve/Reject tail, Select/MultiSelect options, document review, error recovery—reduces copy-paste and keeps keyboard focus visuals consistent.

4. **Unify prompt rendering and cursor metadata:** Consider one function returning `(display_string, Option<byte_cursor>)` or a tiny struct so `draw` and `editing_prompt_cursor_position` cannot diverge.

5. **Simplify `MarkdownPlanActionLayout`:** Either remove `FixedFooterAlways` and encode the “no fixed footer” invariant only in tests (e.g. assert layout function is constant), or make `markdown_plan_action_layout_for_view` meaningful again (if product might revive alternate layouts). As-is, the enum documents history more than it models runtime state.

6. **Factor shared “select highlight after mouse” logic** between `event_loop.rs` and `virtual_tui.rs` into a single helper (inputs: mode, `view_state`, `intent_tx`) to reduce duplicate `matches!(AppMode::Select…)` blocks.

7. **Tune `worktree_display` log levels:** Prefer `debug!`/`trace!` for per-format success paths; reserve `info!` for rare anomalies (e.g. empty display for a non-root path) if those matter operationally.

---

## File references (quick index)

| Area | Path |
|------|------|
| Worktree status segment | `packages/tddy-core/src/presenter/worktree_display.rs` |
| Frame draw, markdown tail, cursor, status bar text, large tests | `packages/tddy-tui/src/render.rs` |
| Spinner vs idle heartbeat, Virtual TUI interval | `packages/tddy-tui/src/status_bar_activity.rs` |
| Local terminal loop, crossterm, `draw` + caret Show | `packages/tddy-tui/src/event_loop.rs` |
| Headless terminal, CSI strip, input/resize/mouse parse, broadcast drain | `packages/tddy-tui/src/virtual_tui.rs` |
| Ratatui unstable feature flag | `packages/tddy-tui/Cargo.toml` |

# Changeset: TUI user prompt strip, footer row, full-height Enter affordance

**Date**: 2026-04-03  
**Status**: Complete — PR wrap validated  
**Type**: Feature

## Affected packages

- `tddy-tui`

## Related feature documentation

- `docs/ft/coder/tui-status-bar.md`
- `docs/ft/web/web-terminal.md`

## Summary

Bottom chrome includes a dedicated footer row under the prompt block. In `Running` mode with non-empty follow-up text, the activity pane shows a last-line strip with white foreground on dark grey. Mouse mode Enter affordance occupies the trailing three columns across the full height of the status bar, prompt block, and footer. `TDDY_E2E_NO_ENTER_AFFORDANCE` suppresses overlay paint for byte-stable tests.

## Technical changes (State B)

- **Layout**: `layout_chunks_with_inbox` produces seven vertical regions; the seventh is `footer_bar` with height one.
- **Render**: `paint_user_prompt_activity_strip` paints the running-input line; `paint_enter_affordance` draws the frame over `enter_button_rect`; empty `Paragraph` fills `footer_bar`.
- **Mouse**: `enter_button_rect` width three; height spans status, prompt, and footer; `handle_mouse_event` uses the same rectangle.

## Acceptance tests (reference)

- `layout_footer_adds_exactly_one_row_to_bottom_chrome`
- `enter_button_rect_geometry_covers_status_prompt_and_footer_three_cols_wide`
- `activity_buffer_user_prompt_strip_white_on_grey`
- `TDDY_E2E_NO_ENTER_AFFORDANCE_skips_enter_overlay_paint`

## Wrapped documentation targets

- `docs/ft/coder/tui-status-bar.md`
- `docs/ft/coder/changelog.md`
- `docs/ft/web/web-terminal.md`
- `docs/ft/web/changelog.md`
- `docs/dev/changesets.md`
- `packages/tddy-tui/docs/architecture.md`
- `packages/tddy-tui/docs/changesets.md`

## Validation Results (PR wrap)

| Step | Result |
|------|--------|
| **1. Validate changes** | Reviewed `layout.rs`, `mouse_map.rs`, `render.rs`, `event_loop.rs`. Hot-path `log::info!` demoted to `trace`/`debug` where layout/paint/caret run every frame; `paint_enter_affordance` inner loop uses `ENTER_BUTTON_COLS` instead of literal `3`. No stray `red-test-output.txt` in tree. |
| **2. Validate tests** | Prior report: `docs/dev/1-WIP/tui-prompt-footer-enter-validate/validate-tests-report.md` (125 `tddy-tui` + 9 `grpc_terminal_rpc` tests). Re-run: `./dev cargo test` workspace — **exit 0**. |
| **3. Production readiness** | `TDDY_E2E_NO_ENTER_AFFORDANCE` comment clarified (any set value skips overlay). `red_phase` remains test-gated stderr only. |
| **4. Clean code** | Affordance paint width unified with `ENTER_BUTTON_COLS` per clean-code note. |
| **5. Final validation** | Same code review pass after edits; no new issues. |
| **6. fmt / clippy / test** | `./dev cargo fmt --all`; `./dev cargo clippy --workspace -- -D warnings`; `./dev cargo test` — **all succeeded**. |
| **7. Documentation** | Changeset section updated; feature docs already wrapped per targets above. |

**Follow-ups (optional):** `ENTER_BUTTON_ROWS` legacy export remains documented in `mouse_map`; optional E2E assertion for env-gate glyphs per validate-tests report.

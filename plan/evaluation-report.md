# Evaluation Report

## Summary

Branch relocates the activity spinner into the status bar before Goal:, adds an 8-hex UUID first-field segment (or em-dash placeholder), plumbs workflow_session_id through PresenterState and presenter_impl (SessionStarted + start_workflow; cleared on completion/restart paths). Top-right spinner overlay removed; Virtual TUI still uses shared draw(). cargo check -p tddy-core -p tddy-tui passes. Risks are mostly low: non-UUID engine session ids always show the placeholder; optional tddy-core integration tests for session lifecycle were not added. Untracked root-level *.json / verify txt artifacts should not be committed.

## Risk Level

low

## Changed Files

- packages/tddy-core/src/presenter/presenter_impl.rs (modified, +19/−0)
- packages/tddy-core/src/presenter/state.rs (modified, +3/−0)
- packages/tddy-tui/src/render.rs (modified, +120/−17)
- packages/tddy-tui/src/ui.rs (modified, +120/−0)
- packages/tddy-tui/src/virtual_tui.rs (modified, +2/−0)
- packages/tddy-tui/tests/error_recovery_apply_event.rs (modified, +1/−0)
- packages/tddy-tui/tests/virtual_tui_ctrl_c_kills_child.rs (modified, +1/−0)

## Affected Tests

- packages/tddy-tui/src/ui.rs: updated
  Unit tests: first_segment_matches_uuid_prefix_before_hyphen, format_status_bar_with_activity_prefix_leads_with_spinner_frame, status_bar_text_orders_spinner_segment_then_goal (in #[cfg(test)] mod).
- packages/tddy-tui/src/render.rs: updated
  Unit test virtual_tui_still_emits_bytes_while_idle; PresenterState initializer gains workflow_session_id.
- packages/tddy-tui/tests/error_recovery_apply_event.rs: updated
  sample_state PresenterState extended with workflow_session_id: None.
- packages/tddy-tui/tests/virtual_tui_ctrl_c_kills_child.rs: updated
  running_presenter_state PresenterState extended with workflow_session_id: None.

## Validity Assessment

The diff matches the PRD: spinner and session segment lead the status line; Goal/State/elapsed/agent tail is unchanged after Goal:; no println/eprintln in tddy-tui src; top-right (area.width-2,0) spinner path is gone. Session id is wired from ProgressEvent::SessionStarted and start_workflow, cleared on terminal workflow outcomes as described in Green. Virtual TUI shares draw() so formatting stays consistent. Remaining gap: UUID-only segment rule may hide prefixes for non-UUID session ids; optional future tddy-core tests for workflow_session_id transitions. Overall the change is valid for the stated use-case.

## Build Results

- tddy-core: pass (./dev cargo check -p tddy-core -p tddy-tui succeeded)
- tddy-tui: pass

## Issues

- [info/product/ux] packages/tddy-tui/src/ui.rs:49: first_hyphen_segment_of_workflow_session_id only displays a segment when the first field is exactly 8 ASCII hex digits. Opaque or non-UUID session ids (e.g. some agent thread formats) always map to SESSION_SEGMENT_PLACEHOLDER, which may under-inform users expecting a visible prefix.
  Suggestion: If engine ids are not always UUID-shaped, document the rule or extend parsing deliberately (with tests).
- [low/hygiene] tddy-green-submit.json: Untracked artifact files at repo root (tddy-*-submit.json, tddy-red-test-output.txt, tddy-green-verify.txt) are not part of the feature; risk of accidental commit.
  Suggestion: Delete or gitignore before merge.

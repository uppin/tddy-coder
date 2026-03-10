# Plan Approval Step — PRD

**Product Area**: Coder
**Status**: Draft
**Created**: 2026-03-10
**Affected Features**: [planning-step.md](../planning-step.md)

## Summary

Introduce a plan approval gate after the plan step completes. The user is presented with three choices: **View** (full-screen scrollable tui-markdown modal showing PRD.md), **Approve** (proceed to acceptance-tests), or **Refine** (free-text feedback that resumes the LLM session for plan refinement). After refinement, the same approval gate re-appears. This creates a review loop until the user explicitly approves.

## Background

Currently, after the plan step produces PRD.md + TODO.md, the workflow automatically transitions to acceptance-tests. The user has no opportunity to review the plan or request changes before implementation begins. This is problematic because:

- Plans may contain incorrect assumptions or miss requirements
- The user cannot see the PRD without leaving the TUI to open the file
- There is no refinement loop — the only option is to restart from scratch

This feature adds a human-in-the-loop checkpoint between planning and implementation.

## Proposed Changes

### What's Changing

1. **New elicitation in workflow_runner**: After plan completes (state = `Planned`), the workflow runner sends a `ClarificationNeeded` event with 3 options (View / Approve / Refine) before proceeding to acceptance-tests.
2. **New `AppMode::PlanReview` in Presenter**: A new mode that renders the 3-option approval menu.
3. **New `AppMode::MarkdownViewer` in Presenter**: A full-screen modal that renders PRD.md using `tui-markdown` with keyboard scrolling (PageUp/PageDown/Up/Down), dismissed via Q or Esc.
4. **Refine flow**: When "Refine" is selected, the TUI enters `TextInput` mode for the user to type feedback. The feedback is sent back to the workflow runner, which resumes the existing plan session with the refinement instructions. After re-planning, the approval gate re-appears.
5. **Plain mode support**: In non-TTY/piped mode, the approval prompt is text-based: display options, read user's choice from stdin.

### What Stays the Same

- The plan task itself (PlanTask, planning.rs prompts, structured output parsing)
- The PRD.md / TODO.md artifact format
- The changeset.yaml structure
- All other workflow steps (acceptance-tests, red, green, etc.)
- The existing ClarificationNeeded/WaitingForInput mechanism (reused for the approval question)

## Requirements

### Plan Approval Gate (workflow_runner level)

1. After plan completes successfully, the workflow_runner sends a plan approval question via `WorkflowEvent::ClarificationNeeded` with 3 options: View, Approve, Refine
2. The workflow_runner blocks on `answer_rx.recv()` waiting for the user's choice
3. **Approve**: Workflow proceeds to acceptance-tests (current behavior)
4. **View**: The presenter enters a markdown viewer mode showing PRD.md. After dismissal, the approval question re-appears
5. **Refine**: The presenter enters text input mode. User types refinement feedback. The workflow_runner resumes the existing plan session with the feedback, re-runs plan, re-parses output, re-writes artifacts, and re-presents the approval gate
6. The approval loop continues until the user selects "Approve"
7. This applies both to the initial plan (fresh workflow) and to plan resume/completion scenarios

### Markdown Viewer (TUI)

1. Full-screen modal (replaces activity log + prompt bar with PRD content)
2. Uses `tui-markdown` crate for rich markdown rendering (headers, bold/italic, lists, code blocks, tables)
3. Keyboard scrolling: Up/Down (1 line), PageUp/PageDown (page)
4. Dismissed with Q or Esc — returns to the plan approval 3-option menu
5. Reads PRD.md content from plan_dir (path available in context)

### Plain Mode Support

1. After plan completes, print: `Plan generated. Options: [v] View  [a] Approve  [r] Refine`
2. **View**: Print PRD.md content to stdout, then re-prompt
3. **Approve**: Proceed to next step
4. **Refine**: Print `Enter refinement feedback:`, read a line, resume plan session

### Refinement Mechanism

1. Resume the existing plan session (same `session_id` from changeset.yaml) with the user's feedback
2. The feedback is formatted as a followup prompt (similar to `build_followup_prompt`)
3. After the LLM responds, re-parse the structured output, re-write PRD.md + TODO.md, update changeset.yaml
4. Re-present the approval gate

### State Machine

1. No new states in changeset.yaml — the approval gate is a presenter/workflow-runner-level concern
2. The plan step still transitions Init → Planning → Planned
3. The approval gate happens between Planned and AcceptanceTesting
4. Refinement re-invokes the plan task but does not change the changeset state (it stays Planned)

### New Dependencies

1. `tui-markdown` crate added to `tddy-tui/Cargo.toml` for markdown rendering

## Impact Analysis

### Technical Impact

- **tddy-core**: New `AppMode` variants (`PlanReview`, `MarkdownViewer`), new `UserIntent` variants, `WorkflowEvent` may need a new variant for plan content, or existing `ClarificationNeeded` can be reused
- **tddy-tui**: New rendering logic for the markdown viewer modal, new key handling for PlanReview and MarkdownViewer modes, `tui-markdown` dependency
- **tddy-coder**: `workflow_runner.rs` gets the approval loop logic; `plain.rs` gets the plain-mode approval prompt

### User Impact

- Users gain the ability to review and refine plans before implementation starts
- The workflow becomes slightly longer (one extra approval step) but provides much better control
- No breaking changes to existing CLI flags or behavior

## Testing Plan

### Test Level

Integration tests — the plan approval is an orchestration concern spanning workflow_runner, presenter, and TUI layers.

### Acceptance Tests

1. **Plan approval: approve proceeds to next step** — After plan completes, selecting "Approve" transitions to acceptance-tests
2. **Plan approval: view shows PRD content then returns to menu** — Selecting "View" shows PRD.md, dismissing returns to the 3-option menu
3. **Plan approval: refine re-runs plan and re-shows approval** — Selecting "Refine", typing feedback, results in plan re-run and approval gate re-appearing
4. **Plan approval: multiple refinements then approve** — Refine → Refine → Approve works correctly
5. **Markdown viewer: scrolling works** — PageUp/PageDown/Up/Down scroll the content
6. **Markdown viewer: Q and Esc dismiss** — Both keys return to the approval menu
7. **Plain mode: approval prompt works** — In piped mode, the 3-option text prompt appears and each option works
8. **Workflow resumes correctly after approval** — After approval, acceptance-tests step runs with correct context

### Target Test Files

- `packages/tddy-coder/tests/cli_integration.rs` (plain mode approval)
- `packages/tddy-coder/tests/presenter_integration.rs` (presenter-level approval flow)
- `packages/tddy-core/tests/planning_integration.rs` (workflow_runner approval loop)

## Acceptance Criteria

- [ ] After plan completes, user sees 3-option menu (View / Approve / Refine)
- [ ] "View" opens full-screen markdown viewer with rendered PRD.md
- [ ] Markdown viewer supports keyboard scrolling (Up/Down/PageUp/PageDown)
- [ ] Q or Esc dismisses the markdown viewer, returns to approval menu
- [ ] "Approve" proceeds to acceptance-tests step
- [ ] "Refine" opens text input, user types feedback
- [ ] Refinement resumes existing plan session with feedback
- [ ] After refinement, PRD.md is re-written and approval gate re-appears
- [ ] Multiple refinement cycles work correctly
- [ ] Plain mode shows text-based approval prompt
- [ ] Plain mode "view" prints PRD.md to stdout
- [ ] Existing tests continue to pass (no regressions)

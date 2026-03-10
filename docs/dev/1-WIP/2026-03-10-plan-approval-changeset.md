# Changeset: Plan Approval Feature

**Date**: 2026-03-10
**Status**: ✅ Wrapped
**Type**: Feature

## Affected Packages

- **tddy-core**: New WorkflowEvent, AppMode, UserIntent variants; planning.rs refinement prompt; PlanTask refinement_feedback handling; workflow_runner approval loop; presenter_impl approval flow
- **tddy-tui**: New view_state fields, key_map handlers, render functions for PlanReview and MarkdownViewer; tui-markdown dependency (ratatui 0.30)
- **tddy-grpc**: Proto + convert for AppMode PlanReview/MarkdownViewer and UserIntent ApprovePlan/ViewPlan/RefinePlan/DismissViewer
- **tddy-coder**: plain.rs read_plan_approval_plain; run.rs plan approval loop for plain mode

## Related Feature Documentation

- [PRD: Plan Approval Step](../../ft/coder/1-WIP/PRD-2026-03-10-plan-approval.md)
- [Planning Step](../../ft/coder/planning-step.md)

## Summary

Add a plan approval gate after the plan step completes. The workflow_runner sends `PlanApprovalNeeded` with PRD content; the user chooses View (full-screen tui-markdown modal), Approve (proceed to acceptance-tests), or Refine (free-text feedback that resumes the LLM session). View is handled locally by the Presenter; Refine re-runs the plan task with refinement feedback. Plain mode gets a text-based approval prompt.

## Background

Currently the workflow transitions directly from plan to acceptance-tests. Users cannot review the PRD in the TUI or request refinements before implementation. This changeset adds a human-in-the-loop checkpoint.

## Scope

- [x] **Core types**: WorkflowEvent::PlanApprovalNeeded, AppMode::PlanReview/MarkdownViewer, UserIntent variants
- [x] **Planning**: build_refinement_prompt, PlanTask refinement_feedback handling
- [x] **Presenter**: PlanApprovalNeeded event handler, intent handlers (ApprovePlan, ViewPlan, RefinePlan, DismissViewer), plan_refinement_pending flag
- [x] **Workflow runner**: Plan approval loop after plan completes (approve/refine/view flow)
- [x] **TUI**: PlanReview and MarkdownViewer rendering, key handling, tui-markdown dependency
- [x] **Plain mode**: read_plan_approval_plain, wire into run.rs
- [x] **Acceptance tests**: Plan approval flow tests (presenter, workflow_runner, plain mode)
- [x] **Documentation**: Update changesets.md when wrapped

## Technical Changes

### State A (Current)

- Plan task completes → workflow advances to acceptance-tests
- No plan review step
- ClarificationNeeded used only for LLM questions

### State B (Target)

- Plan task completes → workflow_runner sends PlanApprovalNeeded
- User sees 3 options: View, Approve, Refine
- View: Presenter enters MarkdownViewer mode (tui-markdown), Q/Esc dismiss
- Approve: workflow proceeds to acceptance-tests
- Refine: TextInput mode → feedback sent to workflow_runner → run_goal("plan") with refinement_feedback → re-show approval
- Plain mode: text prompt with v/a/r options

## Implementation Milestones

- [x] Add WorkflowEvent::PlanApprovalNeeded, AppMode::PlanReview/MarkdownViewer, UserIntent::ApprovePlan/ViewPlan/RefinePlan/DismissViewer
- [x] Add build_refinement_prompt in planning.rs
- [x] PlanTask: check refinement_feedback context, use build_refinement_prompt
- [x] Presenter: handle PlanApprovalNeeded, intents, plan_refinement_pending
- [x] Workflow runner: plan approval loop (read PRD, send event, recv answer, approve/refine handling)
- [x] TUI: view_state, key_map, render for PlanReview and MarkdownViewer
- [x] Add tui-markdown to tddy-tui (ratatui 0.30)
- [x] Plain mode: read_plan_approval_plain, wire into run.rs
- [x] Acceptance tests
- [x] StubBackend: recognize refinement prompt for plan (skip clarification questions)

## Acceptance Tests

1. **Plan approval: approve proceeds** — After plan completes, Approve transitions to acceptance-tests
2. **Plan approval: view then approve** — View shows PRD, dismiss returns to menu, Approve proceeds
3. **Plan approval: refine re-shows approval** — Refine + feedback → plan re-run → approval gate re-appears
4. **Plan approval: multiple refinements** — Refine → Refine → Approve works
5. **Markdown viewer: scroll and dismiss** — PageUp/PageDown/Up/Down scroll; Q/Esc dismiss
6. **Plain mode: approval prompt** — v/a/r options work in piped mode
7. **Workflow resumes after approval** — acceptance-tests runs with correct context

## Technical Debt & Production Readiness

(To be filled during validation)

## Decisions & Trade-offs

- View handled entirely by Presenter (no roundtrip to workflow_runner)
- Refinement uses run_goal("plan", ctx) with new engine instance, same LLM session_id from changeset
- plan_refinement_pending flag distinguishes refinement AnswerText from clarification AnswerText

## References

- Plan document: `.cursor/plans/plan_approval_feature_616843ae.plan.md`

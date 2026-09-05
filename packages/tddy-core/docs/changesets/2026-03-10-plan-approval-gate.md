# 2026-03-10 — Plan Approval Gate

**Type:** Feature

WorkflowEvent::PlanApprovalNeeded; AppMode::PlanReview, MarkdownViewer; UserIntent::ApprovePlan, ViewPlan, RefinePlan, DismissViewer. build_refinement_prompt, PlanTask refinement_feedback handling. workflow_runner approval loop (approve/refine/view) after plan. presenter_impl: PlanApprovalNeeded handler, intent handlers, plan_refinement_pending. StubBackend: recognize refinement prompt, skip clarification. (tddy-core)

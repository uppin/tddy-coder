# 2026-03-10 — Plan Approval Gate

- **Plan approval gate**: After the plan step completes, the user sees a 3-option menu: View (full-screen PRD modal), Approve (proceed to acceptance-tests), or Refine (free-text feedback that resumes the LLM session).
- **Markdown viewer**: Full-screen tui-markdown modal for PRD.md. Keyboard scrolling (Up/Down, PageUp/PageDown). Q or Esc dismisses.
- **Refinement loop**: Refine sends feedback to the plan session; plan re-runs; approval gate re-appears until the user approves.
- **Plain mode**: Text prompt `[v] View  [a] Approve  [r] Refine`; reads choice from stdin.
- **Packages**: tddy-core (WorkflowEvent, AppMode, UserIntent variants; workflow_runner approval loop), tddy-tui (PlanReview/MarkdownViewer rendering, tui-markdown), tddy-coder (plain.rs, run.rs), tddy-grpc (proto intents and modes).

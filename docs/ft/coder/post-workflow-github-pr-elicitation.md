# Post-workflow GitHub PR and worktree elicitation

**Product area:** Coder  
**Updated:** 2026-05-02

## Summary

The workflow engine distinguishes **structured** phases (plan through green) from **terminal completion** (**`WorkflowComplete`**). Immediately before that boundary, the product roadmap calls for **two ordered operator decisions**:

1. **GitHub PR** — open a pull request for the session branch using authenticated GitHub access.
2. **Session worktree** — after automation reaches a **`published`** PR outcome, optionally remove the session git worktree (the worktree-removal prompt is **ineligible** when the operator skips or declines PR automation, when automation **fails**, or when **`github_pr_status.phase`** lacks **`published`**).

Persisted **`changeset.workflow`** fields and **`tddy_tools persist-changeset-workflow`** payloads carry booleans plus a machine-readable **`github_pr_status`** blob so resume, presenters, CLI, gRPC subscribers, and daemon surfaces share identical truth.

### Shipped persistence and policy APIs

These pieces exist today:

| Layer | Responsibility |
|--------|----------------|
| **`changeset-workflow`** schema | Validates **`post_workflow_open_github_pr`**, **`post_workflow_remove_session_worktree`**, **`github_pr_status`** with **`additionalProperties: false`** (see **[workflow-json-schemas.md](workflow-json-schemas.md)**). |
| **`tddy_core::changeset::{ChangesetWorkflow, GithubPrStatus}`** | Serde-backed mirror of **`changeset.yaml`** **`workflow`** JSON. |
| **`merge_persisted_workflow_into_context`** | Exposes **`post_workflow_*`** booleans and **`github_pr_status`** as **`serde_json::Value`** on **`Context`**. |
| **`tddy_core::post_workflow`** | Pure helpers (**`post_workflow_elicitation_step_order`**, **`should_prompt_session_worktree_removal`**, **`should_reprompt_github_pr_on_resume`**, **`post_workflow_pr_status_display_line`**); **`log::trace`/`log::debug`** instrumentation; structured English lines suitable for presenters (callers steer stdout/stderr in TUI to avoid corrupting **[session-layout.md](session-layout.md)** norms). |

### Canonical **`github_pr_status.phase`** strings (policy helpers)

The resume gate treats **`published`**, **`failed`**, **`declined`**, and **`skipped_no_pr`** as terminal: **`should_reprompt_github_pr_on_resume`** returns **`false`**. Non-terminal phases (for example **`in_progress`**) keep reprompt semantics open until orchestration settles state.

Session worktree removal eligibility uses **`published`** strictly: **`should_prompt_session_worktree_removal(user_consented_to_pr, phase)`** is **`true`** only when the operator consented to PR automation **and** **`phase == "published"`**.

### Display strings

**`post_workflow_pr_status_display_line`** maps automation phases to operator-facing text (for example **`pushing_branch`**, **`published`**, **`failed`**, plus a generic fallback for other phase labels). Callers layer this into activity logs, plain CLI, or structured events.

### GitHub automation

Actual REST calls, token requirements, and MCP tool wiring remain under **[github-pr-tools-mcp.md](github-pr-tools-mcp.md)** and **`GITHUB_TOKEN` / `GH_TOKEN`**. The **`github_pr_status`** object records outcomes for observability; failures surface in **`error`** without silent recovery.

### Outstanding product work

Ordered elicitation at **`WorkflowComplete`**, GitHub REST execution with phases written through **`persist-changeset-workflow`**, and session worktree removal follow **`tddy_core::worktree`** conventions. Presenters should align with **`demo_options`** / **`run_optional_step_x`** persistence patterns (same durable **`changeset.yaml`** **`workflow`** shape). End-of-graph **presenter** flows (**`workflow_runner`**, plain mode, Telegram/daemon callback parity) still need to **collect** answers, **invoke** automation, **fan out** status lines across channels, and reflect intermediate **`github_pr_status.phase`** updates in **`changeset.yaml`**. This document describes the **durable contract** and **policy surface** those layers consume.

## Related

- **[workflow-json-schemas.md](workflow-json-schemas.md)** — **`persist-changeset-workflow`** contract  
- **[github-pr-tools-mcp.md](github-pr-tools-mcp.md)** — MCP PR tools and environment  
- **[workflow-recipes.md](workflow-recipes.md)** — recipe topology and goals  
- **[session-layout.md](session-layout.md)** — TUI I/O expectations  

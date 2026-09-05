# 2026-04-04 — TDD interview step before plan

- **Workflow recipes**: **`TddRecipe`** start goal **`interview`**; graphs **`interview` → `plan` → …**; relay **`.workflow/tdd_interview_handoff.txt`** into **`plan`** via **`answers`**; **`goal_requires_tddy_tools_submit`** **`false`** for **`interview`**.
- **Core**: **`WorkflowRecipe::plan_refinement_goal()`** (default **`start_goal()`**); **`TddRecipe`** refinement target **`plan`**; **`GrillMeRecipe`** refinement target **`create-plan`**; **`run_plan_refinement`** in **`tddy-coder`** uses **`plan_refinement_goal()`** for session tag lookup and **`run_goal`**.
- **CLI / hooks**: Session id preservation when a workflow **`session_id`** is already bound in **`before_interview`**, **`before_acceptance_tests`**, **`before_red`**; failed-resume helpers for TDD **`start_goal`** / **`plan`** alignment.
- **Tests**: Integration and recipe acceptance tests for graph topology, handoff, **`backend_invoke_no_tddy_tools_submit`** parity for **`interview`**; e2e presenter tests account for **`Interviewing`** / **`Interviewed`** transitions.
- **Docs**: [workflow-recipes.md](../workflow-recipes.md), [planning-step.md](../planning-step.md), [implementation-step.md](../implementation-step.md); package **`changesets.md`** entries for **tddy-core**, **tddy-coder**, **tddy-workflow-recipes**.

# 2026-03-29 — **Free prompting

**Type:** Feature

invoke graph, hooks, submit policy** — **`FreePromptingRecipe::build_graph`**: single **`BackendInvokeTask`** for **`prompting`** (no **`EndTask`**); **`WorkflowRecipe::goal_requires_tddy_tools_submit`** override for **`prompting`**; **`FreePromptingWorkflowHooks::agent_output_sink`** for **`WorkflowEvent::AgentOutput`**; **`StubBackend`** **`prompting`** response. See [docs/ft/coder/workflow-recipes.md](../../../../../docs/ft/coder/workflow-recipes.md). (tddy-workflow-recipes, tddy-core)

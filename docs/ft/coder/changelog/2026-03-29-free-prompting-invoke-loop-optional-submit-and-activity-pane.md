# 2026-03-29 — Free prompting: invoke loop, optional submit, and activity pane

- **Core**: **`WorkflowRecipe::goal_requires_tddy_tools_submit`** (default **`true`**); **`BackendInvokeTask`** completes a turn from agent **`InvokeResponse::output`** when the recipe opts out for that goal; **`FlowRunner`** maps **`Continue`** with no next graph task to **`WaitingForInput`** so a single-node graph can take another user line without **`EndTask`**.
- **Recipes**: **`FreePromptingRecipe`** graph is one **`BackendInvokeTask`** for **`prompting`**; **`goal_requires_tddy_tools_submit`** is **`false`** for **`prompting`**; **`FreePromptingWorkflowHooks::agent_output_sink`** emits **`WorkflowEvent::AgentOutput`** for streaming assistant text to the TUI activity pane.
- **Stub**: **`StubBackend`** **`response_for_goal`** includes a **`prompting`** arm for deterministic tests.
- **Docs**: [workflow-recipes.md](../workflow-recipes.md).

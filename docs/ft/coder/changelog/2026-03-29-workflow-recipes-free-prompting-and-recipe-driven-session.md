# 2026-03-29 — Workflow recipes: free-prompting and recipe-driven session document policy

- **Recipes**: **`FreePromptingRecipe`** (**`free-prompting`**): **Prompting** loop with start goal **`prompting`**; **`uses_primary_session_document`** **`false`** (no PRD-style primary-document approval gate on that path).
- **Registry**: **`tddy-workflow-recipes::approval_policy`** (**`supported_workflow_recipe_cli_names`**, **`recipe_should_skip_session_document_approval`**); **`recipe_resolve`** resolves **`free-prompting`**; **`unknown_workflow_recipe_error`** enumerates supported CLI names.
- **Bootstrap**: Presenter **`workflow_runner`** **`run_start_goal_without_output_dir`** writes **`recipe`** into **`changeset.yaml`** when creating the session directory (same field as daemon **`StartSession`** and CLI session bootstrap).
- **CLI / TUI / web**: **`--recipe free-prompting`**; **`workflow_recipe_selection_question`** includes **Free prompting**; **`recipe_cli_name_from_selection_label`** maps the label to **`free-prompting`**.
- **Tests**: **`workflow_recipe_acceptance`**, **`recipe_policy_red`**, **`presenter_integration`** acceptance cases for TDD document approval, bugfix **`reproduce`**, and free-prompting without **DocumentReview**; **`grpc_terminal_rpc`** UTF-8-safe assertion previews.
- **Docs**: [workflow-recipes.md](../workflow-recipes.md).

# 2026-04-05 — Default `free-prompting` and `/start-<recipe>`

**Type:** Feature

**`feature_start_slash`**: parse, menu labels, remainder handling, post-**`WorkflowComplete`** return to **`free-prompting`** for structured **`/start-*`** sessions; **`agent_skills`**: **`SlashMenuItem::StartRecipe`**, **`slash_menu_entries`** ordering; presenter **`try_handle_start_slash_line`**, **`finish_start_slash_structured_run_if_needed`**, **`start_slash_structured_run_active`**; session bootstrap surfaces **`write_changeset`** failures where applicable. Feature docs: [workflow-recipes.md](../../../../../docs/ft/coder/workflow-recipes.md), [feature-prompt-agent-skills.md](../../../../../docs/ft/coder/feature-prompt-agent-skills.md). (tddy-core, tddy-tui, tddy-coder, tddy-workflow-recipes)

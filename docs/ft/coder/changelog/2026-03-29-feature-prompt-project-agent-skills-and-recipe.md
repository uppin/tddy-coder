# 2026-03-29 — Feature prompt: project agent skills and `/recipe`

- **Core**: **`tddy_core::agent_skills`** scans **`.agents/skills/<folder>/SKILL.md`**, parses YAML frontmatter (**`name`**, **`description`**), rejects folder/name mismatches with **`InvalidSkillEntry`**, exposes **`slash_menu_items`** (**`BuiltinRecipe`** plus valid skills), **`compose_prompt_with_selected_skill`** (PRD-shaped block with path and body), and **`agents_skills_scan_cache_token`** for scan invalidation hints.
- **Presenter**: **`apply_feature_slash_builtin_recipe`**, **`recipe_slash_selection_active`**, **`with_recipe_resolver`**; **`workflow_recipe_selection_question`** and **`recipe_cli_name_from_selection_label`** in **`backend`**. **`tddy-coder`** **`run.rs`** attaches the resolver for daemon and full TUI presenters.
- **Tests**: **`prompt_slash_skills_acceptance`**, **`prompt_slash_skills_lower`**, unit tests in **`agent_skills.rs`**.
- **Docs**: [Feature prompt: agent skills](../feature-prompt-agent-skills.md), [overview](../1-OVERVIEW.md), **`packages/tddy-core/docs/architecture.md`**.

# Feature prompt: project agent skills and `/recipe`

**Product area**: Coder (tddy-coder, tddy-core)  
**Status**: Library, presenter, and TUI slash menu (**`/recipe`**, **`/chain`**, **`/start-…`**, skills); default compose is **reference-only** (no inlined `SKILL.md` body).

## Summary

The workflow discovers **project skills** under **`.agents/skills/<skill-folder>/SKILL.md`** (YAML frontmatter with **`name`** and **`description`**). Valid skills appear in the feature-prompt slash menu after built-in **`/recipe`**, the **`/chain`** row, the shipped **`/start-…`** rows, and each valid skill folder (see [Workflow recipes](workflow-recipes.md)). In the **TUI feature field**, choosing a skill inserts a compact **`/<skill-name>`** token (one logical unit for navigation and Backspace), drawn with a **dark-navy** background fill and **white** label text on the token. On **Enter** / submit, that token is expanded to the **agent-facing reference string** (fully-qualified **`@.agents/skills/<skill-name>`** tag, path to **`SKILL.md`**, “not inlined” instruction, and **`User request:`** with surrounding free text) via **`compose_prompt_skill_reference`** — the markdown body is **not** inlined (avoids prompt bloat). Selecting **`/recipe`** drives **workflow recipe selection** (TDD vs Bugfix) through the presenter when the user confirms an option. The **`/chain`** row inserts the literal **`/chain`** label for stacked-session flows (same **`SlashMenuEntry::StartRecipe`** mechanism as **`/start-…`**).

## On-disk layout

- Root: **`.agents/skills/`** (constant **`AGENTS_SKILLS_DIR`** in **`tddy_core::agent_skills`**).
- Each immediate child directory is a skill folder; **`SKILL.md`** inside it holds YAML frontmatter closed by **`---`** lines, then markdown body.
- **`name`** in frontmatter equals the parent folder name for a skill to be **valid**. Mismatch produces an **invalid** scan record with a human-readable reason; that skill is omitted from the valid list and slash menu.

## Public API (tddy-core)

Re-exported from **`tddy_core`**:

| Item | Role |
|------|------|
| **`scan_skills_at_project_root`** | Returns **`SkillScanReport`** (**`valid`**, **`invalid`**) for a project root path. |
| **`slash_menu_items`** | Returns **`SlashMenuItem`** values: **`BuiltinRecipe`**, **`StartRecipe { label }`** for **`/chain`** then each shipped **`/start-…`** label, plus **`Skill { name }`** for each valid discovery. |
| **`compose_prompt_skill_reference`** | Default outbound prompt: **`@.agents/skills/<name>`** tag, path to **`SKILL.md`**, explicit “not inlined” instruction, **`User request:`** tail. |
| **`compose_prompt_with_selected_skill`** | Optional **full inline** compose (fenced `SKILL.md` body) for backends that cannot read the repo. |
| **`parse_skill_frontmatter`**, **`folder_name_matches_frontmatter_name`** | Parsing and folder/name checks for tests and tooling. |
| **`agents_skills_scan_cache_token`** | Directory metadata token for **`.agents/skills`** (callers use it to bound repeated scans). |

## Composed prompt shape (default)

The reference string includes:

1. A line starting with **`[Skill: @.agents/skills/<name> — explicit selection]`** (fully-qualified, agent-agnostic).  
2. A sentence naming the relative **`SKILL.md`** path under the project root.  
3. An explicit note that the skill body is **not** inlined.  
4. A **`User request:`** section followed by the user’s free text.

**Inline variant** ([**`compose_prompt_with_selected_skill`**](#public-api-tddy-core)): same header style as before, plus a fenced block with the full markdown body after frontmatter.

## Built-in `/recipe`, `/chain`, and `/start-…`

- **`/start-…`** rows correspond to shipped workflow recipes (**`tdd`**, **`tdd-small`**, **`bugfix`**, **`free-prompting`**, **`grill-me`**). Submitting a **`/start-<cli>`** line selects that recipe, updates **`changeset.yaml`**, and restarts the workflow (see **Feature prompt: `/start-<recipe>`** in [Workflow recipes](workflow-recipes.md)).
- **`/chain`** is a **`StartRecipe`**-shaped slash row for stacked-session entry (label **`/chain`**); it aligns with the **`/chain-workflow`** Telegram command at the product level. The Virtual TUI uses **`chain_workflow_parent_picker_active`** while the parent-picker step is active; **`ViewState::on_mode_changed`** sets that flag to false whenever **`AppMode`** is not **`FeatureInput`**. Telegram **`/chain-workflow`** uses **`tcp:`** parent-picker callbacks and the same **`sessions_base`** listing rules; scope for deeper Virtual TUI vs Telegram parity is described in **[Telegram session control](../daemon/telegram-session-control.md)** and **[Session chaining follow-ups](../../dev/1-WIP/2026-05-02-changeset-session-chaining.md)**.
- **`Presenter::apply_feature_slash_builtin_recipe`** (only from **`AppMode::FeatureInput`**) arms a single-select question built by **`workflow_recipe_selection_question`** for labeled recipe options.
- **`Presenter::with_recipe_resolver`** supplies a resolver from **`tddy-coder`** (**`resolve_workflow_recipe_from_cli_name`**) so the active **`WorkflowRecipe`** matches the chosen CLI name.
- **`Presenter::recipe_slash_selection_active`** is true while that selection UI is active.

## Automated tests

| Location | Focus |
|----------|--------|
| **`packages/tddy-core/src/agent_skills.rs`** (`#[cfg(test)]`) | Frontmatter parsing, folder/name match, cache token when the skills directory exists. |
| **`packages/tddy-coder/tests/prompt_slash_skills_acceptance.rs`** | Discovery, mismatch quarantine, menu contents, composition literals, recipe slash presenter mode. |
| **`packages/tddy-coder/tests/prompt_slash_skills_lower.rs`** | Menu with no skills directory; non-empty invalid reasons. |

## Related documentation

- [Workflow recipes](workflow-recipes.md) — recipe CLI names, defaults, and **`/start-<recipe>`** behavior.  
- [Coder overview](1-OVERVIEW.md) — product capabilities table.  
- **`packages/tddy-core/docs/architecture.md`** — presenter and **`agent_skills`** module notes.

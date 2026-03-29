# Feature prompt: project agent skills and `/recipe`

**Product area**: Coder (tddy-coder, tddy-core)  
**Status**: Library and presenter support; the ratatui feature input omits slash completion wiring.

## Summary

The workflow discovers **project skills** under **`.agents/skills/<skill-folder>/SKILL.md`** (YAML frontmatter with **`name`** and **`description`**). Valid skills appear in a logical slash menu model beside a built-in **`/recipe`** entry. Selecting a skill yields a **composed feature string** that includes the skill name, the relative path to **`SKILL.md`**, and the markdown body so downstream agents can follow the skill without opening the file separately. Selecting **`/recipe`** drives **workflow recipe selection** (TDD vs Bugfix) through the presenter when the user confirms an option.

## On-disk layout

- Root: **`.agents/skills/`** (constant **`AGENTS_SKILLS_DIR`** in **`tddy_core::agent_skills`**).
- Each immediate child directory is a skill folder; **`SKILL.md`** inside it holds YAML frontmatter closed by **`---`** lines, then markdown body.
- **`name`** in frontmatter equals the parent folder name for a skill to be **valid**. Mismatch produces an **invalid** scan record with a human-readable reason; that skill is omitted from the valid list and slash menu.

## Public API (tddy-core)

Re-exported from **`tddy_core`**:

| Item | Role |
|------|------|
| **`scan_skills_at_project_root`** | Returns **`SkillScanReport`** (**`valid`**, **`invalid`**) for a project root path. |
| **`slash_menu_items`** | Returns **`SlashMenuItem`** values: **`BuiltinRecipe`** plus **`Skill { name }`** for each valid discovery. |
| **`compose_prompt_with_selected_skill`** | Builds the outbound prompt block (header, path line, fenced body, **`User request:`** tail). |
| **`parse_skill_frontmatter`**, **`folder_name_matches_frontmatter_name`** | Parsing and folder/name checks for tests and tooling. |
| **`agents_skills_scan_cache_token`** | Directory metadata token for **`.agents/skills`** (callers use it to bound repeated scans). |

## Composed prompt shape

The composed string includes:

1. A line starting with **`[Skill: <name> — explicit invocation]`**.  
2. A sentence naming the selected skill and the path to **`SKILL.md`**.  
3. Instructions to follow the skill for the turn.  
4. A fenced block containing the **markdown body** (content after frontmatter).  
5. A **`User request:`** section followed by the user’s free text.

## Built-in `/recipe`

- **`Presenter::apply_feature_slash_builtin_recipe`** (only from **`AppMode::FeatureInput`**) arms a single-select question built by **`workflow_recipe_selection_question`** (**`TDD`** / **`Bugfix`** labels).
- **`Presenter::with_recipe_resolver`** supplies a resolver from **`tddy-coder`** (**`resolve_workflow_recipe_from_cli_name`**) so the active **`WorkflowRecipe`** matches the chosen CLI name (**`tdd`** / **`bugfix`**).
- **`Presenter::recipe_slash_selection_active`** is true while that selection UI is active.

## Automated tests

| Location | Focus |
|----------|--------|
| **`packages/tddy-core/src/agent_skills.rs`** (`#[cfg(test)]`) | Frontmatter parsing, folder/name match, cache token when the skills directory exists. |
| **`packages/tddy-coder/tests/prompt_slash_skills_acceptance.rs`** | Discovery, mismatch quarantine, menu contents, composition literals, recipe slash presenter mode. |
| **`packages/tddy-coder/tests/prompt_slash_skills_lower.rs`** | Menu with no skills directory; non-empty invalid reasons. |

## Related documentation

- [Workflow recipes](workflow-recipes.md) — selectable **`tdd`** / **`bugfix`** recipes at CLI and session level.  
- [Coder overview](1-OVERVIEW.md) — product capabilities table.  
- **`packages/tddy-core/docs/architecture.md`** — presenter and **`agent_skills`** module notes.

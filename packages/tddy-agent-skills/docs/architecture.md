# tddy-agent-skills architecture

## Overview

Project agent skills and the feature-prompt slash contract. A leaf: no workspace dependencies.

`tddy-core` re-exports this crate whole (`pub use tddy_agent_skills::*;`), so every `tddy_core::{agent_skills, feature_start_slash}::…` path consumers name resolves unchanged. New code should name `tddy_agent_skills` directly.

## Agent skills (`agent_skills`)

- **Purpose**: Discover Cursor-style project skills under **`.agents/skills/<folder>/SKILL.md`**, validate YAML frontmatter (**`name`**, **`description`**) against the folder name, build slash-menu entries (**`SlashMenuItem::BuiltinRecipe`** plus skills), and compose the outbound user prompt after skill selection (**`compose_prompt_with_selected_skill`**).
- **Scan**: `scan_skills_at_project_root` walks immediate subdirectories of **`.agents/skills`**, reads **`SKILL.md`**, classifies into **`DiscoveredSkill`** or **`InvalidSkillEntry`**.
- **Cache hint**: `agents_skills_scan_cache_token` exposes directory mtime for callers that cache scan results.
- **Exports**: Module is public; key symbols are re-exported from this crate's root (and so from `tddy_core`'s) for **`tddy-coder`** and tests.
- **Feature doc**: [feature-prompt-agent-skills.md](../../../docs/ft/coder/feature-prompt-agent-skills.md).


## Feature start slash (`feature_start_slash`)

- The `/start-<recipe>` feature-prompt commands and `DEFAULT_UNSPECIFIED_WORKFLOW_RECIPE_CLI_NAME`
  (`free-prompting`, the recipe a feature runs under when none is selected).
- Kept below the presenter and `tddy-workflow-recipes` so both, and
  `agent_skills::slash_menu_entries`, share one contract without a circular dependency.

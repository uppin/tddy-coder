//! Agent skills: discovering `SKILL.md` files under a project and composing them into prompts,
//! plus the `/start-<recipe>` slash command that begins a feature.
//!
//! Extracted from `tddy-core`, which re-exports every module and root item at its old path.

pub mod agent_skills;
pub mod feature_start_slash;

pub use agent_skills::{
    agents_skills_scan_cache_token, compose_prompt_skill_reference,
    compose_prompt_with_selected_skill, folder_name_matches_frontmatter_name,
    parse_skill_frontmatter, read_skill_markdown_body_for_compose, scan_skills_at_project_root,
    slash_menu_entries, slash_menu_items, DiscoveredSkill, InvalidSkillEntry,
    ParsedSkillFrontmatter, SkillMdParseError, SkillScanReport, SlashMenuEntry, SlashMenuItem,
    AGENTS_SKILLS_DIR,
};
pub use feature_start_slash::{
    feature_slash_menu_start_command_labels,
    next_session_recipe_cli_name_after_start_slash_structured_workflow_complete,
    parse_feature_start_slash_line, remainder_after_start_slash_line,
    DEFAULT_UNSPECIFIED_WORKFLOW_RECIPE_CLI_NAME, SHIPPED_WORKFLOW_RECIPE_CLI_NAMES,
};

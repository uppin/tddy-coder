//! Lower-level integration tests for prompt slash / agent skills (PRD red phase).
//! Targets `tddy_core::agent_skills` without full presenter workflow.

mod common;

use std::fs;
use std::path::PathBuf;

use tddy_core::agent_skills::{self, SlashMenuItem};
use tddy_core::{scan_skills_at_project_root, slash_menu_items};

fn temp_project_root(label: &str) -> PathBuf {
    let base =
        std::env::temp_dir().join(format!("tddy-slash-lower-{}-{}", label, std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("mkdir temp project");
    base
}

fn write_skill_md(
    project_root: &std::path::Path,
    folder: &str,
    frontmatter_name: &str,
    description: &str,
    body_markdown: &str,
) {
    let dir = project_root
        .join(agent_skills::AGENTS_SKILLS_DIR)
        .join(folder);
    fs::create_dir_all(&dir).expect("mkdir skill dir");
    let content = format!(
        "---\nname: {frontmatter_name}\ndescription: {description}\n---\n\n{body_markdown}\n"
    );
    fs::write(dir.join("SKILL.md"), content).expect("write SKILL.md");
}

/// **slash_menu_lists_builtin_when_agents_skills_missing** (granular)
#[test]
fn slash_menu_lists_builtin_when_agents_skills_missing() {
    let root = temp_project_root("no_agents_skills");
    let items = slash_menu_items(&root);
    assert!(
        items
            .iter()
            .any(|i| matches!(i, SlashMenuItem::BuiltinRecipe)),
        "with no .agents/skills, menu must still list built-in /recipe; got {items:?}"
    );
}

/// **scan_records_invalid_skill_with_non_empty_reason** (granular)
#[test]
fn scan_records_invalid_skill_with_non_empty_reason() {
    let root = temp_project_root("invalid_reason");
    write_skill_md(&root, "foo", "bar", "mismatch", "## X\n");

    let report = scan_skills_at_project_root(&root);
    let inv = report
        .invalid
        .iter()
        .find(|e| e.folder_name == "foo")
        .expect("folder/name mismatch must yield an invalid skill record");
    assert!(
        !inv.reason.is_empty(),
        "invalid skill must carry a human-readable reason; got {:?}",
        inv
    );
}

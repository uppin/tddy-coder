//! Acceptance tests: feature-prompt slash menu, `.agents/skills` discovery, and prompt composition (PRD).
//!
//! These tests target APIs in `tddy_core::agent_skills` and presenter hooks; implementations are
//! stubs until the feature ships — expect **red** (assertions fail).

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use serial_test::serial;
use std::sync::Arc;
use tddy_coder::{AppMode, Presenter};
use tddy_core::{
    agent_skills::{self, SlashMenuItem},
    compose_prompt_skill_reference, compose_prompt_with_selected_skill,
    scan_skills_at_project_root, slash_menu_items,
};
use tddy_workflow_recipes::TddRecipe;

fn temp_project_root(label: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "tddy-slash-skills-{}-{}",
        label,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("mkdir temp project");
    base
}

/// Writes `.agents/skills/<folder>/SKILL.md` with YAML frontmatter and body.
fn write_skill_md(
    project_root: &Path,
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

fn presenter_with_recipe() -> Presenter {
    Presenter::new("stub", "default", Arc::new(TddRecipe))
}

/// **skill_discovery_loads_agents_skills_directories**
#[test]
#[serial]
fn skill_discovery_loads_agents_skills_directories() {
    let root = temp_project_root("discover");
    write_skill_md(
        &root,
        "foo",
        "foo",
        "Short description for slash subtitle.",
        "## UniqueHeading\n",
    );

    let report = scan_skills_at_project_root(&root);
    assert_eq!(
        report.valid.len(),
        1,
        "expected exactly one valid skill for foo/SKILL.md with matching name; got {:?}",
        report
    );
    assert_eq!(report.valid[0].name, "foo");
    assert!(
        !report.valid[0].description.is_empty(),
        "description must be non-empty for menu subtitle"
    );
    assert!(
        report.invalid.is_empty(),
        "valid fixture must not produce invalid entries: {:?}",
        report.invalid
    );
}

/// **skill_frontmatter_rejects_name_folder_mismatch**
#[test]
#[serial]
fn skill_frontmatter_rejects_name_folder_mismatch() {
    let root = temp_project_root("mismatch");
    write_skill_md(
        &root,
        "foo",
        "bar",
        "wrong name for folder foo",
        "## Body\n",
    );

    let report = scan_skills_at_project_root(&root);
    assert!(
        !report.valid.iter().any(|s| s.name == "bar"),
        "must never expose mismatched frontmatter name as a valid skill: {:?}",
        report.valid
    );
    assert!(
        report.invalid.iter().any(|e| e.folder_name == "foo"),
        "expected invalid skill record for folder foo (name bar in file); valid={:?} invalid={:?}",
        report.valid,
        report.invalid
    );
}

/// **slash_menu_lists_builtin_recipe_and_skills**
#[test]
#[serial]
fn slash_menu_lists_builtin_recipe_and_skills() {
    let root = temp_project_root("slash_menu");
    write_skill_md(&root, "foo", "foo", "Skill desc", "## SkillContent\n");

    let items = slash_menu_items(&root);
    assert!(
        items
            .iter()
            .any(|i| matches!(i, SlashMenuItem::BuiltinRecipe)),
        "slash menu must include built-in /recipe; got {:?}",
        items
    );
    assert!(
        items
            .iter()
            .any(|i| matches!(i, SlashMenuItem::Skill { name } if name == "foo")),
        "slash menu must list discovered skill foo; got {:?}",
        items
    );
}

/// **composed_prompt_tags_skill_and_path_without_inlining_body** (default feature-prompt behavior)
#[test]
#[serial]
fn composed_prompt_tags_skill_and_path_without_inlining_body() {
    let out = compose_prompt_skill_reference("foo", ".agents/skills/foo/SKILL.md", "Add login.");
    assert!(
        out.contains("[Skill: @.agents/skills/foo"),
        "composed prompt must use fully-qualified @.agents/skills/<name>; got:\n{out}"
    );
    assert!(
        out.contains(".agents/skills/foo/SKILL.md"),
        "composed prompt must name skill path for the agent to read; got:\n{out}"
    );
    assert!(
        out.contains("The skill body is **not** inlined"),
        "composed prompt must state skill body is not inlined; got:\n{out}"
    );
    assert!(
        !out.contains("UniqueSkillBodyMarker"),
        "composed prompt must not paste SKILL.md body; got:\n{out}"
    );
    assert!(
        out.contains("Add login."),
        "composed prompt must retain user request; got:\n{out}"
    );
}

/// Full inline compose remains available for backends that cannot read the repo.
#[test]
#[serial]
fn compose_prompt_with_selected_skill_still_inlines_body_when_requested() {
    let body = "## UniqueSkillBodyMarker\nDo the thing.\n";
    let out = compose_prompt_with_selected_skill(
        "foo",
        ".agents/skills/foo/SKILL.md",
        body,
        "Add login.",
    );
    assert!(
        out.contains("UniqueSkillBodyMarker"),
        "inline mode pastes body"
    );
    assert!(out.contains("Add login."));
}

/// **recipe_slash_triggers_recipe_selection_intent_or_mode**
#[test]
#[serial]
fn recipe_slash_triggers_recipe_selection_intent_or_mode() {
    let mut presenter = presenter_with_recipe();
    assert!(
        matches!(presenter.state().mode, AppMode::FeatureInput),
        "precondition: feature input mode"
    );

    presenter.apply_feature_slash_builtin_recipe();

    let active = presenter.recipe_slash_selection_active();
    let mode = presenter.state().mode.clone();
    assert!(
        active || matches!(mode, AppMode::Select { .. }),
        "after accepting /recipe from slash menu, presenter must arm recipe selection (recipe_slash_selection_active) or enter Select mode; active={active} mode={mode:?}"
    );
}

//! Project Agent Skills under `.agents/skills/<name>/SKILL.md` and feature-prompt slash UX (PRD).
//!
//! Discovery, YAML frontmatter parsing, slash menu entries, and prompt composition.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Relative path segment for skill roots (per PRD / Agent Skills layout).
pub const AGENTS_SKILLS_DIR: &str = ".agents/skills";

/// A skill accepted for slash menu and prompt injection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSkill {
    pub name: String,
    pub description: String,
}

/// Record when a skill directory or `SKILL.md` fails validation (e.g. folder/name mismatch).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidSkillEntry {
    pub folder_name: String,
    pub skill_md_path: PathBuf,
    pub reason: String,
}

/// Result of scanning `.agents/skills` at a project root.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillScanReport {
    pub valid: Vec<DiscoveredSkill>,
    pub invalid: Vec<InvalidSkillEntry>,
}

/// Built-in slash commands vs discovered skills (clear separation for UI and tests).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashMenuItem {
    BuiltinRecipe,
    Skill { name: String },
}

/// One row in the feature-prompt slash menu (labels + skill descriptions for the TUI).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashMenuEntry {
    BuiltinRecipe,
    Skill { name: String, description: String },
}

/// Minimal `name` / `description` from SKILL.md YAML frontmatter (between `---` lines).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSkillFrontmatter {
    pub name: String,
    pub description: String,
}

/// Errors from SKILL.md frontmatter extraction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillMdParseError {
    MissingOpeningDelimiter,
    MissingClosingDelimiter,
    Yaml(String),
    MissingName,
    MissingDescription,
}

#[derive(Debug, Deserialize)]
struct FrontmatterYaml {
    name: Option<String>,
    description: Option<String>,
}

/// Split first YAML frontmatter block from SKILL.md source (`---` … `---`).
fn split_skill_frontmatter_blocks(raw: &str) -> Result<(&str, &str), SkillMdParseError> {
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let rest = raw
        .strip_prefix("---")
        .ok_or(SkillMdParseError::MissingOpeningDelimiter)?;
    let rest = rest
        .strip_prefix('\n')
        .or_else(|| rest.strip_prefix("\r\n"))
        .ok_or(SkillMdParseError::MissingOpeningDelimiter)?;

    let (close_len, yaml_end) = if let Some(i) = rest.find("\n---\n") {
        (5, i)
    } else if let Some(i) = rest.find("\n---\r\n") {
        (6, i)
    } else {
        return Err(SkillMdParseError::MissingClosingDelimiter);
    };

    let yaml_src = &rest[..yaml_end];
    let body = &rest[yaml_end + close_len..];
    Ok((yaml_src, body))
}

/// Extract `name` and `description` from SKILL.md source.
pub fn parse_skill_frontmatter(
    skill_md_source: &str,
) -> Result<ParsedSkillFrontmatter, SkillMdParseError> {
    log::debug!(
        "parse_skill_frontmatter: input_len={}",
        skill_md_source.len()
    );
    let (yaml_src, _body) = split_skill_frontmatter_blocks(skill_md_source)?;
    let fm: FrontmatterYaml = serde_yaml::from_str(yaml_src).map_err(|e| {
        log::debug!("parse_skill_frontmatter: yaml error: {e}");
        SkillMdParseError::Yaml(e.to_string())
    })?;
    let name = fm.name.unwrap_or_default().trim().to_string();
    let description = fm.description.unwrap_or_default().trim().to_string();
    if name.is_empty() {
        log::debug!("parse_skill_frontmatter: missing name");
        return Err(SkillMdParseError::MissingName);
    }
    if description.is_empty() {
        log::debug!("parse_skill_frontmatter: missing description");
        return Err(SkillMdParseError::MissingDescription);
    }
    log::info!(
        "parse_skill_frontmatter: ok name={} desc_len={}",
        name,
        description.len()
    );
    Ok(ParsedSkillFrontmatter { name, description })
}

/// Whether `folder_name` matches the frontmatter `name` for Agent Skills layout rules.
///
/// Returns [`None`] when either side is empty (no meaningful comparison).
pub fn folder_name_matches_frontmatter_name(folder_name: &str, name: &str) -> Option<bool> {
    if folder_name.is_empty() || name.is_empty() {
        log::debug!(
            "folder_name_matches_frontmatter_name: empty input folder={folder_name:?} name={name:?}"
        );
        return None;
    }
    let matches = folder_name == name;
    log::debug!(
        "folder_name_matches_frontmatter_name: folder={folder_name} name={name} -> {matches}"
    );
    Some(matches)
}

/// Hint for cache invalidation: filesystem metadata token for `.agents/skills` (mtime or missing).
pub fn agents_skills_scan_cache_token(project_root: &Path) -> Option<u64> {
    let skills_root = project_root.join(AGENTS_SKILLS_DIR);
    let meta = fs::metadata(&skills_root).ok()?;
    if !meta.is_dir() {
        log::debug!(
            "agents_skills_scan_cache_token: path exists but not a dir {}",
            skills_root.display()
        );
        return None;
    }
    let modified = meta.modified().ok()?;
    let secs = modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    log::debug!(
        "agents_skills_scan_cache_token: {} -> {secs}",
        skills_root.display()
    );
    Some(secs)
}

fn scan_one_skill_dir(skills_root: &Path, folder_name: String, report: &mut SkillScanReport) {
    let skill_dir = skills_root.join(&folder_name);
    let skill_md_path = skill_dir.join("SKILL.md");
    if !skill_md_path.is_file() {
        log::debug!(
            "scan_one_skill_dir: skip {} (no SKILL.md)",
            skill_dir.display()
        );
        return;
    }
    let raw = match fs::read_to_string(&skill_md_path) {
        Ok(s) => s,
        Err(e) => {
            log::info!(
                "scan_one_skill_dir: read failed {}: {e}",
                skill_md_path.display()
            );
            report.invalid.push(InvalidSkillEntry {
                folder_name: folder_name.clone(),
                skill_md_path,
                reason: format!("failed to read SKILL.md: {e}"),
            });
            return;
        }
    };
    let fm = match parse_skill_frontmatter(&raw) {
        Ok(f) => f,
        Err(e) => {
            let reason = format!("invalid frontmatter: {e:?}");
            log::info!(
                "scan_one_skill_dir: invalid frontmatter for folder={folder_name}: {reason}"
            );
            report.invalid.push(InvalidSkillEntry {
                folder_name: folder_name.clone(),
                skill_md_path,
                reason,
            });
            return;
        }
    };
    match folder_name_matches_frontmatter_name(&folder_name, &fm.name) {
        Some(true) => {
            log::info!(
                "scan_one_skill_dir: accepted skill `{}` from {}",
                fm.name,
                skill_md_path.display()
            );
            report.valid.push(DiscoveredSkill {
                name: fm.name,
                description: fm.description,
            });
        }
        Some(false) => {
            let reason = format!(
                "frontmatter name {:?} does not match folder name {:?}",
                fm.name, folder_name
            );
            log::info!("scan_one_skill_dir: rejected {reason}");
            report.invalid.push(InvalidSkillEntry {
                folder_name,
                skill_md_path,
                reason,
            });
        }
        None => {
            let reason =
                "folder name or frontmatter name is empty; cannot validate Agent Skills layout"
                    .to_string();
            report.invalid.push(InvalidSkillEntry {
                folder_name,
                skill_md_path,
                reason,
            });
        }
    }
}

/// Scan `.agents/skills/<name>/SKILL.md` under `project_root` and classify valid vs invalid skills.
pub fn scan_skills_at_project_root(project_root: &Path) -> SkillScanReport {
    let skills_root = project_root.join(AGENTS_SKILLS_DIR);
    log::debug!(
        "scan_skills_at_project_root: root={} skills_dir={}",
        project_root.display(),
        skills_root.display()
    );
    let mut report = SkillScanReport::default();
    let entries = match fs::read_dir(&skills_root) {
        Ok(e) => e,
        Err(e) => {
            log::info!(
                "scan_skills_at_project_root: no readable skills dir ({}): {e}",
                skills_root.display()
            );
            return report;
        }
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let folder_name = entry.file_name().to_string_lossy().into_owned();
        scan_one_skill_dir(&skills_root, folder_name, &mut report);
    }
    report.valid.sort_by(|a, b| a.name.cmp(&b.name));
    log::info!(
        "scan_skills_at_project_root: valid={} invalid={}",
        report.valid.len(),
        report.invalid.len()
    );
    report
}

/// Entries shown when the user types `/` in the feature prompt: built-ins plus valid skills.
pub fn slash_menu_entries(project_root: &Path) -> Vec<SlashMenuEntry> {
    log::debug!(
        "slash_menu_entries: project_root={}",
        project_root.display()
    );
    let scan = scan_skills_at_project_root(project_root);
    let mut items = Vec::with_capacity(1 + scan.valid.len());
    items.push(SlashMenuEntry::BuiltinRecipe);
    for s in scan.valid {
        items.push(SlashMenuEntry::Skill {
            name: s.name,
            description: s.description,
        });
    }
    log::info!("slash_menu_entries: total entries={}", items.len());
    items
}

/// Same ordering as [`slash_menu_entries`], as [`SlashMenuItem`] (for tests and callers that only need ids).
pub fn slash_menu_items(project_root: &Path) -> Vec<SlashMenuItem> {
    slash_menu_entries(project_root)
        .into_iter()
        .map(|e| match e {
            SlashMenuEntry::BuiltinRecipe => SlashMenuItem::BuiltinRecipe,
            SlashMenuEntry::Skill { name, .. } => SlashMenuItem::Skill { name },
        })
        .collect()
}

/// Read `.agents/skills/<skill_name>/SKILL.md` and return the markdown body after YAML frontmatter.
pub fn read_skill_markdown_body_for_compose(
    project_root: &Path,
    skill_name: &str,
) -> Result<String, String> {
    let path = project_root
        .join(AGENTS_SKILLS_DIR)
        .join(skill_name)
        .join("SKILL.md");
    let raw = fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let (_yaml, body) = split_skill_frontmatter_blocks(&raw).map_err(|e| format!("{e:?}"))?;
    Ok(body.to_string())
}

/// Short outbound prompt after the user picks a skill: **fully-qualified `@` tag + file path**, no body.
///
/// Uses **`@.agents/skills/<skill-name>`** so any agent can treat it as a stable, repo-root-relative skill
/// pointer (compare `@path` context pulls in IDEs) without inlining `SKILL.md`.
///
/// `skill_md_relative_path` must be the concrete file path (typically **`.agents/skills/<name>/SKILL.md`**).
pub fn compose_prompt_skill_reference(
    skill_name: &str,
    skill_md_relative_path: &str,
    user_request: &str,
) -> String {
    log::debug!(
        "compose_prompt_skill_reference: skill={skill_name} path={skill_md_relative_path} user_len={}",
        user_request.len()
    );
    let at_skill_ref = format!("@{AGENTS_SKILLS_DIR}/{skill_name}");
    let out = format!(
        "[Skill: {at_skill_ref} — explicit selection]\n\
         The user selected this fully-qualified skill reference. Read `{skill_md_relative_path}` under the workflow project root and follow it. The skill body is **not** inlined in this prompt.\n\
         \n\
         User request:\n\
         \n\
         {}",
        user_request.trim_end()
    );
    log::info!("compose_prompt_skill_reference: composed_len={}", out.len());
    out
}

/// Build the outbound feature string with the full `SKILL.md` body inlined (large prompts).
///
/// Prefer [`compose_prompt_skill_reference`] when the agent can read files under the project root.
pub fn compose_prompt_with_selected_skill(
    skill_name: &str,
    skill_md_relative_path: &str,
    skill_md_body: &str,
    user_request: &str,
) -> String {
    log::debug!(
        "compose_prompt_with_selected_skill: skill={skill_name} path={skill_md_relative_path} body_len={} user_len={}",
        skill_md_body.len(),
        user_request.len()
    );
    let body = skill_md_body.trim();
    let out = format!(
        "[Skill: {skill_name} — explicit invocation]\n\
         The user selected project skill `{skill_name}` from {skill_md_relative_path}.\n\
         Follow these instructions for this turn and until the skill scope is complete:\n\
         \n\
         ---\n\
         {body}\n\
         ---\n\
         \n\
         User request:\n\
         \n\
         {user_request}"
    );
    log::info!(
        "compose_prompt_with_selected_skill: composed_len={}",
        out.len()
    );
    out
}

#[cfg(test)]
mod agent_skills_unit_red {
    use super::*;

    /// Lower-level red: parser must return structured frontmatter for valid SKILL.md.
    #[test]
    fn parse_skill_frontmatter_parses_minimal_yaml() {
        let src = "---\nname: foo\ndescription: Short desc\n---\n\n## Body\n";
        let got =
            parse_skill_frontmatter(src).expect("expected Ok frontmatter for valid stub input");
        assert_eq!(got.name, "foo");
        assert_eq!(got.description, "Short desc");
    }

    /// Lower-level red: matching folder and `name` field must classify as match.
    #[test]
    fn folder_name_matches_frontmatter_accepts_matching_pair() {
        assert_eq!(
            folder_name_matches_frontmatter_name("foo", "foo"),
            Some(true),
            "folder foo and name foo must match"
        );
    }

    /// Lower-level red: mismatch must not be treated as match.
    #[test]
    fn folder_name_matches_frontmatter_rejects_mismatch() {
        assert_eq!(
            folder_name_matches_frontmatter_name("foo", "bar"),
            Some(false),
            "folder foo with name bar must not match"
        );
    }

    #[test]
    fn compose_prompt_skill_reference_uses_at_qualified_skill_path() {
        let out = compose_prompt_skill_reference(
            "my-skill",
            ".agents/skills/my-skill/SKILL.md",
            "Ship the feature.",
        );
        assert!(out.contains("[Skill: @.agents/skills/my-skill"));
        assert!(out.contains(".agents/skills/my-skill/SKILL.md"));
        assert!(out.contains("Ship the feature."));
        assert!(
            !out.contains("---\n"),
            "reference compose must not use fenced skill body blocks"
        );
    }

    /// Lower-level red: cache token should exist when `.agents/skills` is present on disk.
    #[test]
    fn agents_skills_scan_cache_token_some_when_skills_dir_exists() {
        let root =
            std::env::temp_dir().join(format!("tddy-agent-skills-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(AGENTS_SKILLS_DIR).join("x")).expect("mkdir");
        let token = agents_skills_scan_cache_token(&root);
        assert!(
            token.is_some(),
            "expected cache token when .agents/skills exists; got {token:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}

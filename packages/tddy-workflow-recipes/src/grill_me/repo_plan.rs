//! Default path for a **version-controlled** grill-me brief under the working tree (`plans/`), per AGENTS.md.

use std::path::{Path, PathBuf};

/// Why [`persisted_grill_me_brief_path`] refused a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrillMePersistedBriefPathError {
    /// `plan_stem` was empty or only whitespace.
    EmptyPlanStem,
    /// `plan_stem` contained a path separator or `..`.
    InvalidPlanStem,
}

/// Resolves `repo_root/plans/<plan_stem>.md` for persisting a grill-me **Create plan** brief.
///
/// `plan_stem` must be a single path segment (no `/`, `\`, or `..`). Use a descriptive slug
/// (e.g. `my-feature-grill-brief`); see **AGENTS.md** **Documentation Hierarchy** for defaults.
pub fn persisted_grill_me_brief_path(
    repo_root: &Path,
    plan_stem: &str,
) -> Result<PathBuf, GrillMePersistedBriefPathError> {
    let stem = plan_stem.trim();
    if stem.is_empty() {
        return Err(GrillMePersistedBriefPathError::EmptyPlanStem);
    }
    if stem.contains('/') || stem.contains('\\') {
        return Err(GrillMePersistedBriefPathError::InvalidPlanStem);
    }
    if stem == ".." {
        return Err(GrillMePersistedBriefPathError::InvalidPlanStem);
    }
    Ok(repo_root.join("plans").join(format!("{stem}.md")))
}

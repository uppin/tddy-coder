//! Default session action manifests for the acceptance-tests goal (`<session>/actions/*.yaml`).
//!
//! Ensures three scope manifests exist so agents and CI can `tddy-tools list-actions` /
//! `invoke-action` before submit (PRD).

use std::fs;
use std::path::Path;

use log::{debug, info};

/// Basename → YAML body for the three required scopes (valid [`tddy_core::session_actions::ActionManifest`]).
const TEMPLATE_MANIFESTS: [(&str, &str); 3] = [
    (
        "acceptance-single-test.yaml",
        r#"version: 1
id: acceptance-single-test
summary: Run one named test by filter (single-test scope)
architecture: native
command:
  - /bin/true
"#,
    ),
    (
        "acceptance-selected-tests.yaml",
        r#"version: 1
id: acceptance-selected-tests
summary: Run only tests selected for this acceptance-tests goal (selected acceptance scope)
architecture: native
command:
  - /bin/true
"#,
    ),
    (
        "acceptance-package-tests.yaml",
        r#"version: 1
id: acceptance-package-tests
summary: Run full package or crate test suite for each affected package (package scope)
architecture: native
command:
  - /bin/true
"#,
    ),
];

/// Ensures the three canonical scope manifests exist under `session_dir/actions/`.
///
/// Missing files are written; existing files are left unchanged so the agent or operator can
/// replace commands without this hook overwriting on every resume.
pub fn ensure_acceptance_tests_session_action_templates(session_dir: &Path) -> std::io::Result<()> {
    let actions_dir = session_dir.join("actions");
    fs::create_dir_all(&actions_dir)?;
    info!(
        target: "tddy_workflow_recipes::tdd::acceptance_tests_action_templates",
        "ensure_acceptance_tests_session_action_templates: session_dir={}",
        session_dir.display()
    );

    let mut wrote = 0usize;
    for (basename, yaml) in TEMPLATE_MANIFESTS {
        let path = actions_dir.join(basename);
        if path.is_file() {
            debug!(
                target: "tddy_workflow_recipes::tdd::acceptance_tests_action_templates",
                "template manifest already present path={}",
                path.display()
            );
            continue;
        }
        fs::write(&path, yaml)?;
        wrote += 1;
        info!(
            target: "tddy_workflow_recipes::tdd::acceptance_tests_action_templates",
            "wrote default session action manifest path={}",
            path.display()
        );
    }

    let n = count_action_manifest_yaml(session_dir)?;
    debug!(
        target: "tddy_workflow_recipes::tdd::acceptance_tests_action_templates",
        "ensure_acceptance_tests_session_action_templates done yaml_files={} files_written_this_call={}",
        n,
        wrote
    );
    Ok(())
}

/// Count `*.yaml` / `*.yml` files directly under `session_dir/actions/`.
pub fn count_action_manifest_yaml(session_dir: &Path) -> std::io::Result<usize> {
    let actions_dir = session_dir.join("actions");
    let rd = match fs::read_dir(&actions_dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e),
    };
    Ok(rd
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .map(|ext| ext == "yaml" || ext == "yml")
                .unwrap_or(false)
        })
        .count())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn ensure_acceptance_tests_session_action_templates_materializes_three_yaml() {
        let dir = tempdir().expect("tempdir");
        ensure_acceptance_tests_session_action_templates(dir.path()).expect("hook ok");
        let n = count_action_manifest_yaml(dir.path()).expect("count");
        assert!(
            n >= 3,
            "expected at least three manifests under actions/; got {n}"
        );
    }
}

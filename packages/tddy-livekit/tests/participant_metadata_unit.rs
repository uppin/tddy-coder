//! Lower-level RED tests for project registry → metadata count (no LiveKit I/O).

use serde::Serialize;
use std::path::Path;
use tddy_livekit::{owned_project_count_for_projects_dir, OWNED_PROJECT_COUNT_METADATA_KEY};

#[derive(Serialize)]
struct ProjectsFile {
    projects: Vec<ProjectRow>,
}

#[derive(Serialize)]
struct ProjectRow {
    project_id: String,
    name: String,
    git_url: String,
    main_repo_path: String,
}

fn write_projects_yaml(dir: &Path, n: usize) -> anyhow::Result<()> {
    let projects: Vec<ProjectRow> = (0..n)
        .map(|i| ProjectRow {
            project_id: format!("unit-proj-{i}"),
            name: format!("Unit {i}"),
            git_url: format!("https://example.com/u-{i}.git"),
            main_repo_path: format!("/tmp/unit-{i}"),
        })
        .collect();
    let yaml = serde_yaml::to_string(&ProjectsFile { projects })?;
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("projects.yaml"), yaml)?;
    Ok(())
}

#[test]
fn owned_project_count_for_projects_dir_matches_projects_yaml_row_count() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let expected: u64 = 4;
    write_projects_yaml(tmp.path(), expected as usize).expect("write projects.yaml");
    let got = owned_project_count_for_projects_dir(tmp.path()).expect("count");
    assert_eq!(
        got,
        expected,
        "{} must equal read_projects row count for {}",
        OWNED_PROJECT_COUNT_METADATA_KEY,
        tmp.path().display()
    );
}

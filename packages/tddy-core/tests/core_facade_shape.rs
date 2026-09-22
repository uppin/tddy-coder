//! The shape `tddy-core` is left in once every group has moved out.
//!
//! The target is a **wiring point**: each group of code moves to a crate of its own, in one
//! dependency direction, every receiving crate stays within the size budget the `#carve` stack is
//! carving to, and `tddy-core` keeps only `pub use` facades. The moved suites prove behaviour; these
//! prove the move happened, and in the right shape.
//!
//! Production lines are counted as the `#carve` size target defines them: everything before the
//! first `#[cfg(test)]` that opens a `mod`, with `*_tests.rs`, `tests.rs` and `test_util.rs`
//! excluded.

use std::path::{Path, PathBuf};

/// The ceiling every crate this stack moves code into is held to.
const PRODUCTION_LINE_BUDGET: usize = 10_000;
/// What `tddy-core` may keep: `lib.rs`, the facades, `ssh_exec` and `test_support`.
const WIRING_POINT_BUDGET: usize = 200;

/// The receivers, bottom of the dependency order first. A receiver may depend only on those before
/// it — that order is what opened the `backend → toolcall → session_actions → changeset → workflow`
/// cycle.
const RECEIVERS_IN_DEPENDENCY_ORDER: [&str; 10] = [
    "tddy-workflow",
    "tddy-log",
    "tddy-agent-skills",
    "tddy-changeset",
    "tddy-session-worktree",
    "tddy-session-actions",
    "tddy-toolcall",
    "tddy-agent-backend",
    "tddy-workflow-engine",
    "tddy-presenter",
];

// ---------------------------------------------------------------------------------------------
// Reading the tree
// ---------------------------------------------------------------------------------------------

fn package(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the packages directory")
        .join(name)
}

fn source_of(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn rust_files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries {
            let path = entry.expect("a readable directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }
    found
}

fn is_test_file(path: &Path) -> bool {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    stem.ends_with("_tests") || stem == "tests" || stem == "test_util"
}

/// The lines of one file before its test module opens.
fn production_part(source: &str) -> Vec<&str> {
    let lines: Vec<&str> = source.lines().collect();
    let end = lines
        .windows(2)
        .position(|pair| {
            let next = pair[1].trim_start();
            let opens_a_test_module = next.starts_with("mod ")
                || next.starts_with("pub mod ")
                || next.starts_with("pub(crate) mod ");
            pair[0].trim() == "#[cfg(test)]" && opens_a_test_module
        })
        .unwrap_or(lines.len());
    lines[..end].to_vec()
}

fn production_lines_of_crate(name: &str) -> usize {
    rust_files_under(&package(name).join("src"))
        .into_iter()
        .filter(|path| !is_test_file(path))
        .map(|path| production_part(&source_of(&path)).len())
        .sum()
}

/// The `[dependencies]` table of a crate's manifest, without its dev- or build-dependencies.
fn normal_dependencies_of(name: &str) -> String {
    source_of(&package(name).join("Cargo.toml"))
        .split("\n[")
        .find(|table| table.starts_with("dependencies]"))
        .unwrap_or_default()
        .to_string()
}

/// Whether `dependencies` names the crate `name` as a dependency key — not merely as a prefix of
/// another crate's name (`tddy-workflow` inside `tddy-workflow-engine`).
fn names_dependency(dependencies: &str, name: &str) -> bool {
    dependencies.lines().any(|line| {
        let key = line.split('=').next().unwrap_or_default().trim();
        key == name
    })
}

/// A line that defines behaviour rather than re-exporting it.
fn defines_something(line: &str) -> bool {
    let line = line.trim_start();
    if line.starts_with("//") {
        return false;
    }
    [
        "fn ",
        "pub fn ",
        "pub(crate) fn ",
        "struct ",
        "pub struct ",
        "enum ",
        "pub enum ",
        "impl ",
        "impl<",
        "trait ",
        "pub trait ",
        "const ",
        "pub const ",
        "static ",
        "pub static ",
        "macro_rules!",
    ]
    .iter()
    .any(|start| line.starts_with(start))
}

fn defined_in_crate(crate_name: &str, definition: &str) -> bool {
    rust_files_under(&package(crate_name).join("src"))
        .into_iter()
        .any(|path| source_of(&path).contains(definition))
}

fn manifest_of(crate_name: &str) -> String {
    let manifest = source_of(&package(crate_name).join("Cargo.toml"));
    assert!(
        !manifest.is_empty(),
        "`packages/{crate_name}` has no manifest — the crate does not exist yet"
    );
    manifest
}

// ---------------------------------------------------------------------------------------------
// AC1 — the wiring point
// ---------------------------------------------------------------------------------------------

#[test]
fn tddy_core_is_only_a_wiring_point() {
    // Given tddy-core's production sources, less the two files allowed to hold code
    let allowed_to_define = ["ssh_exec.rs", "test_support.rs"];
    let definitions: Vec<String> = rust_files_under(&package("tddy-core").join("src"))
        .into_iter()
        .filter(|path| !is_test_file(path))
        .filter(|path| {
            let file = path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or_default();
            !allowed_to_define.contains(&file)
        })
        .flat_map(|path| {
            let source = source_of(&path);
            production_part(&source)
                .into_iter()
                .filter(|line| defines_something(line))
                .map(|line| format!("{}: {}", path.display(), line.trim()))
                .collect::<Vec<_>>()
        })
        .collect();

    // When the crate is measured
    let production_lines = production_lines_of_crate("tddy-core");

    // Then nothing is defined there any more, and what is left is the size of a facade
    assert!(
        definitions.is_empty(),
        "tddy-core still defines {} items — first: {:?}",
        definitions.len(),
        definitions.iter().take(5).collect::<Vec<_>>()
    );
    assert!(
        production_lines <= WIRING_POINT_BUDGET,
        "tddy-core has {production_lines} production lines; a wiring point has at most {WIRING_POINT_BUDGET}"
    );
}

// ---------------------------------------------------------------------------------------------
// AC2 — the size budget
// ---------------------------------------------------------------------------------------------

#[test]
fn every_crate_receiving_code_stays_within_10k_production_lines() {
    // Given every crate tddy-core's code moves into
    let measured: Vec<(&str, usize)> = RECEIVERS_IN_DEPENDENCY_ORDER
        .into_iter()
        .map(|name| (name, production_lines_of_crate(name)))
        .collect();

    // Then each exists, and none has grown past the budget the stack is carving to
    let missing: Vec<&str> = measured
        .iter()
        .filter(|(_, lines)| *lines == 0)
        .map(|(name, _)| *name)
        .collect();
    assert!(missing.is_empty(), "no production code in: {missing:?}");
    let over_budget: Vec<String> = measured
        .iter()
        .filter(|(_, lines)| *lines > PRODUCTION_LINE_BUDGET)
        .map(|(name, lines)| format!("{name}: {lines}"))
        .collect();
    assert!(
        over_budget.is_empty(),
        "over the {PRODUCTION_LINE_BUDGET}-line budget: {over_budget:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// AC3 — dead code goes
// ---------------------------------------------------------------------------------------------

/// `workflow/mod.rs` declares these as inline modules re-exporting `tddy_graph`, so the files were
/// never compiled — 1,090 lines nobody could have been running.
#[test]
fn the_never_compiled_workflow_files_are_gone() {
    // Given the six files the inline shims shadowed
    let workflow = package("tddy-core").join("src/workflow");
    let dead = [
        "context.rs",
        "graph.rs",
        "hooks.rs",
        "runner.rs",
        "session.rs",
        "task.rs",
    ];

    // When each is looked for
    let still_there: Vec<&str> = dead
        .into_iter()
        .filter(|file| workflow.join(file).exists())
        .collect();

    // Then none is left
    assert!(
        still_there.is_empty(),
        "never-compiled files remain: {still_there:?}"
    );
}

#[test]
fn the_unused_futures_dependency_is_dropped() {
    // Given tddy-core's normal dependencies
    let dependencies = normal_dependencies_of("tddy-core");

    // Then `futures`, which no source file uses, is not among them
    assert!(
        !names_dependency(&dependencies, "futures"),
        "tddy-core still depends on `futures`, which nothing in it uses"
    );
}

// ---------------------------------------------------------------------------------------------
// The two cuts that open the cycle
// ---------------------------------------------------------------------------------------------

/// Cut 1: the only thing the backend took from the workflow module was these two plain types.
#[test]
fn the_recipe_hints_are_defined_by_the_workflow_vocabulary_crate() {
    for definition in ["pub enum PermissionHint", "pub struct GoalHints"] {
        assert!(
            defined_in_crate("tddy-workflow", definition),
            "`tddy-workflow` does not define `{definition}`"
        );
        assert!(
            !defined_in_crate("tddy-core", definition),
            "`tddy-core` still defines `{definition}` rather than re-exporting it"
        );
    }
}

/// Cut 2: the only changeset item that named `WorkflowRecipe` moves up to the engine.
#[test]
fn the_session_continue_goal_is_chosen_by_the_workflow_engine() {
    let definition = "pub fn start_goal_for_session_continue";
    assert!(
        defined_in_crate("tddy-workflow-engine", definition),
        "`tddy-workflow-engine` does not define `start_goal_for_session_continue`"
    );
    assert!(
        !defined_in_crate("tddy-changeset", definition),
        "`tddy-changeset` defines `start_goal_for_session_continue`, which names `WorkflowRecipe`"
    );
}

// ---------------------------------------------------------------------------------------------
// AC4, AC5 — the dependency direction
// ---------------------------------------------------------------------------------------------

#[test]
fn the_agent_backend_does_not_depend_on_the_workflow_engine() {
    // Given the backend crate's manifest
    manifest_of("tddy-agent-backend");
    let dependencies = normal_dependencies_of("tddy-agent-backend");

    // Then the edge that closed the cycle is gone
    assert!(
        !names_dependency(&dependencies, "tddy-workflow-engine"),
        "`tddy-agent-backend` depends on `tddy-workflow-engine`"
    );
}

#[test]
fn the_changeset_crate_does_not_depend_on_the_workflow_engine() {
    // Given the changeset crate's manifest
    manifest_of("tddy-changeset");
    let dependencies = normal_dependencies_of("tddy-changeset");

    // Then the changeset sits below the engine that runs recipes over it
    assert!(
        !names_dependency(&dependencies, "tddy-workflow-engine"),
        "`tddy-changeset` depends on `tddy-workflow-engine`"
    );
}

#[test]
fn no_receiving_crate_depends_on_tddy_core() {
    // Given every receiver's manifest
    let dependents: Vec<&str> = RECEIVERS_IN_DEPENDENCY_ORDER
        .into_iter()
        .filter(|name| {
            manifest_of(name);
            names_dependency(&normal_dependencies_of(name), "tddy-core")
        })
        .collect();

    // Then none reaches back into the facade it was carved out of
    assert!(dependents.is_empty(), "depend on tddy-core: {dependents:?}");
}

#[test]
fn each_receiver_depends_only_on_crates_below_it() {
    // Given each receiver and the receivers above it in the order
    let upward_edges: Vec<String> = RECEIVERS_IN_DEPENDENCY_ORDER
        .iter()
        .enumerate()
        .flat_map(|(position, name)| {
            manifest_of(name);
            let dependencies = normal_dependencies_of(name);
            RECEIVERS_IN_DEPENDENCY_ORDER[position + 1..]
                .iter()
                .filter(|above| names_dependency(&dependencies, above))
                .map(|above| format!("{name} → {above}"))
                .collect::<Vec<_>>()
        })
        .collect();

    // Then no edge points up the order — which is what keeps the old cycle from re-forming
    assert!(upward_edges.is_empty(), "upward edges: {upward_edges:?}");
}

// ---------------------------------------------------------------------------------------------
// AC6 — heavy dependencies leave with their modules
// ---------------------------------------------------------------------------------------------

#[test]
fn tddy_core_no_longer_names_the_heavy_dependencies_itself() {
    // Given tddy-core's normal dependencies
    let dependencies = normal_dependencies_of("tddy-core");

    // Then the crates only one group needed left with that group
    let still_named: Vec<&str> = ["agent-client-protocol", "tokio-util", "jsonschema"]
        .into_iter()
        .filter(|dependency| names_dependency(&dependencies, dependency))
        .collect();
    assert!(
        still_named.is_empty(),
        "tddy-core still names {still_named:?} itself"
    );
}

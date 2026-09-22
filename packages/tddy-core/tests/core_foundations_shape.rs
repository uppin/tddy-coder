//! What `#carve` 5/11 delivers, asserted against the source tree.
//!
//! Three of this node's four claims are about *structure* — which module holds which type, how many
//! fields a struct has, whether a cycle still exists — and none of them is observable from the type
//! system once the code compiles. A cycle between two modules of one crate compiles perfectly well;
//! that is why `tddy-core` has six of them.
//!
//! So these read the source. The measure of production lines is everything before the first
//! `#[cfg(test)]`, matching `docs/dev/todo/2026-09-12-the-two-new-service-rs-files-are-over-budget.md`.

use std::path::{Path, PathBuf};

/// The crate each module group read here moved into when `tddy-core` became a wiring point, keyed
/// by the group's top-level path under `src/`.
const HOMES: [(&str, &str); 1] = [("changeset", "tddy-changeset")];

fn src(relative: &str) -> PathBuf {
    let group = relative.split(['/', '.']).next().unwrap_or_default();
    let home = HOMES
        .iter()
        .find(|(moved, _)| *moved == group)
        .map_or("tddy-core", |(_, home)| home);
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the packages directory")
        .join(home)
        .join("src")
        .join(relative)
}

fn read(relative: &str) -> String {
    let path = src(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()))
}

/// Every `.rs` file under `directory`, at any depth.
fn rust_files_under(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let entries = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("reading {}: {error}", directory.display()));
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            found.extend(rust_files_under(&path));
        } else if path.extension().is_some_and(|it| it == "rs") {
            found.push(path);
        }
    }
    found
}

/// Everything before the first `#[cfg(test)]`.
fn production(text: &str) -> String {
    match text
        .lines()
        .position(|line| line.trim_start().starts_with("#[cfg(test)]"))
    {
        Some(at) => text.lines().take(at).collect::<Vec<_>>().join("\n"),
        None => text.to_string(),
    }
}

/// AC1 — the two `backend` edges a shared DTO created are gone.
///
/// Each was one to four symbols wide, and every one of them was a plain data type living inside a
/// behaviour module: `stream/mod.rs:9` needed `ClarificationQuestion` and `QuestionOption`,
/// `toolcall/client_wire.rs` needed `QuestionOption`, and `workflow/` needed `WorkflowEvent` twice.
#[test]
fn no_module_reaches_into_a_behaviour_module_for_a_shared_dto() {
    // Given the two edges that reached into `backend` for a DTO
    let edges = [
        ("stream/mod.rs", "crate::backend::"),
        ("toolcall/client_wire.rs", "crate::backend::"),
    ];

    // When each is looked for in production code
    let remaining: Vec<&str> = edges
        .into_iter()
        .filter(|(file, path)| production(&read(file)).contains(path))
        .map(|(file, _)| file)
        .collect();

    // Then none reaches for a DTO through the module that happened to define it
    assert!(
        remaining.is_empty(),
        "these still reach into `backend` for a shared DTO: {remaining:?}"
    );
}

/// AC1 — `workflow/` no longer reaches into `presenter` for `WorkflowEvent`.
#[test]
fn the_workflow_engine_does_not_reach_into_the_presenter() {
    // Given every module of the workflow engine
    let modules = rust_files_under(&src("workflow"));
    assert!(
        !modules.is_empty(),
        "no workflow modules were found — the test would pass vacuously"
    );

    // When each is read
    let reaching: Vec<String> = modules
        .iter()
        .filter(|path| {
            let text = std::fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
            production(&text).contains("crate::presenter::")
        })
        .map(|path| path.display().to_string())
        .collect();

    // Then none of them names the presenter
    assert!(
        reaching.is_empty(),
        "the workflow engine still reaches into the presenter: {reaching:?}"
    );
}

/// AC3 — `backend/mod.rs` publishes its own surface, not the workflow's.
///
/// `backend/mod.rs:230-231` re-exported `GoalId` and the recipe trio. A facade like that is why the
/// `backend ↔ workflow` edge looked structural when it was a convenience.
#[test]
fn the_backend_module_no_longer_re_exports_the_workflow_vocabulary() {
    // Given the backend's own module root
    let text = production(&read("backend/mod.rs"));

    // Then it re-exports nothing at all from the workflow, in any spelling
    let republished: Vec<&str> = text
        .lines()
        .map(str::trim_start)
        .filter(|line| line.starts_with("pub use crate::workflow::"))
        .collect();
    assert!(
        republished.is_empty(),
        "`backend` still re-exports the workflow's vocabulary: {republished:?}"
    );
}

/// AC4/AC5 — `changeset.rs` becomes a directory, with the PR-stack model in its own module.
///
/// The file held **two unrelated data models**: `Stack`/`StackNode` at 44–277 and `Changeset` from
/// 278. `#carve` 9/9 needs the first as its own module.
#[test]
fn the_changeset_splits_into_its_four_concerns() {
    // Given the four modules the split produces
    let modules = ["stack", "model", "io", "merge"];

    // When each is looked for
    let absent: Vec<&str> = modules
        .into_iter()
        .filter(|module| !src(&format!("changeset/{module}.rs")).exists())
        .collect();

    // Then none is missing
    assert!(
        absent.is_empty(),
        "these changeset concerns are not yet their own modules: {absent:?}"
    );
}

/// AC4 — the parent keeps a facade and little else, so no consumer is edited.
#[test]
fn the_changeset_parent_is_a_facade() {
    // Given the parent after the split
    let text = production(&read("changeset.rs"));
    let lines = text.lines().count();

    // Then it is small, and it publishes rather than defines
    assert!(
        lines < 200,
        "`changeset.rs` is still {lines} production lines, so the split did not happen"
    );
    assert!(
        !text.contains("pub struct Changeset {"),
        "the parent still defines `Changeset` instead of publishing it"
    );
}

/// AC4 — no module the split produces is over budget.
#[test]
fn no_changeset_module_exceeds_four_hundred_production_lines() {
    // Given all four modules — each must exist, or `read` panics rather than passing vacuously
    let over: Vec<String> = ["stack", "model", "io", "merge"]
        .into_iter()
        .map(|module| {
            (
                module,
                production(&read(&format!("changeset/{module}.rs")))
                    .lines()
                    .count(),
            )
        })
        .filter(|(_, lines)| *lines > 400)
        .map(|(module, lines)| format!("{module} at {lines}"))
        .collect();

    // Then none is over
    assert!(over.is_empty(), "over 400 production lines: {over:?}");
}

/// AC6 — `Presenter` holds seven fields: the five groups, plus what belongs to none of them.
///
/// The 37 fields were never arbitrary — the struct's own doc comments group them. This is that
/// grouping made explicit, and it is what `#carve` 8/9 partitions the 46 methods along.
#[test]
fn the_presenter_holds_five_state_groups_and_nothing_loose() {
    // Given the struct declaration
    let text = read("presenter/presenter_impl.rs");
    let declaration = text
        .split_once("pub struct Presenter {")
        .expect("`Presenter` is declared")
        .1
        .split_once("\n}")
        .expect("the declaration ends")
        .0;

    // When its fields are counted
    let fields = declaration
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with(|c: char| c.is_ascii_lowercase()) && trimmed.contains(':')
        })
        .count();

    // Then only the five groups and the two that belong to none remain
    assert_eq!(
        fields, 7,
        "`Presenter` holds {fields} fields, not the five groups plus `state` and `tddy_data_dir`"
    );
}

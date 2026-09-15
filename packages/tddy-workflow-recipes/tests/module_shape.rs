//! The shape `#carve` 2/9 delivers: one module per parser phase, and the TDD hooks split by
//! lifecycle half.
//!
//! These assertions are the node's contract. They read the tree rather than the type system
//! deliberately — a module that exists but is never named would satisfy a compile-time check, and
//! what this node promises is a *layout* a reader can navigate, with no file over the budget the
//! repo's own backlog sets at 500 production lines.
//!
//! "Production lines" means everything before the first `#[cfg(test)]`, which is the measure
//! `docs/dev/todo/2026-09-12-the-two-new-service-rs-files-are-over-budget.md` uses. Measuring total
//! lines instead would call `session_agent_status.rs` over budget when it is more than half test
//! module, which is exactly the mistake that entry records.

use std::path::{Path, PathBuf};

/// Everything before the first `#[cfg(test)]`, the measure the file-budget convention uses.
fn production_lines(path: &Path) -> usize {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));

    text.lines()
        .position(|line| line.trim_start().starts_with("#[cfg(test)]"))
        .unwrap_or_else(|| text.lines().count())
}

fn src(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(relative)
}

/// The budget the repo's backlog sets, and this node's AC6.
const BUDGET: usize = 500;

/// AC1 — each of the six parser phases is its own module.
///
/// Nothing couples them: each owns its output struct, its private `Structured…` mirror, its `…De`
/// deserialization mirrors and one `parse_*_response`. They share a file because they were written
/// one after another, and the only symbol crossing every seam is `ParseError`.
#[test]
fn each_parser_phase_is_its_own_module() {
    // Given the six phases the parser actually has
    let phases = [
        "planning",
        "acceptance_tests",
        "analyze",
        "green",
        "red",
        "evaluate",
    ];

    // When each is looked for as its own module
    let absent: Vec<&str> = phases
        .into_iter()
        .filter(|phase| !src(&format!("parser/{phase}.rs")).exists())
        .collect();

    // Then none is missing
    assert!(
        absent.is_empty(),
        "these parser phases are not yet their own modules: {absent:?}"
    );
}

/// AC1 — `parser.rs` keeps only what every phase shares, behind a facade.
///
/// The facade is what keeps all nine external `tddy_workflow_recipes::parser::…` reference sites
/// resolving, so not one of them is edited.
#[test]
fn the_parser_parent_keeps_only_the_shared_error_and_a_facade() {
    // Given the parent after the split
    let parent = src("parser.rs");
    let text = std::fs::read_to_string(&parent)
        .unwrap_or_else(|error| panic!("reading {}: {error}", parent.display()));

    // Then it still owns the one symbol every phase needs
    assert!(
        text.contains("ParseError"),
        "`ParseError` is shared by every phase and must stay in the parent"
    );

    // And it publishes the phases rather than holding them
    assert!(
        text.contains("pub use"),
        "the parent declares no facade, so existing `parser::` paths would stop resolving"
    );
    assert!(
        !text.contains("pub fn parse_planning_response"),
        "the parent still defines a phase parser instead of publishing it"
    );
}

/// AC3 — each hooks file keeps only its struct, its inherent impl and `impl RunnerHooks`.
///
/// The phase functions are **free** functions that only `impl RunnerHooks` calls, which is what
/// makes each half separable from the other.
#[test]
fn the_tdd_hooks_split_by_lifecycle_half() {
    // Given both hooks files
    let halves = [
        "tdd/hooks/before.rs",
        "tdd/hooks/after.rs",
        "tdd_small/hooks/before.rs",
        "tdd_small/hooks/after.rs",
    ];

    // When each half is looked for
    let absent: Vec<&str> = halves
        .into_iter()
        .filter(|half| !src(half).exists())
        .collect();

    // Then none is missing
    assert!(
        absent.is_empty(),
        "these hook halves are not yet their own modules: {absent:?}"
    );
}

/// AC6 — no file this node produces exceeds the budget.
///
/// The budget is what the node is for. A split that leaves one module at 900 lines has moved the
/// problem rather than solved it.
#[test]
fn no_module_this_node_produces_exceeds_the_budget() {
    // Given every module the split produces
    let produced = [
        "parser.rs",
        "parser/planning.rs",
        "parser/acceptance_tests.rs",
        "parser/analyze.rs",
        "parser/green.rs",
        "parser/red.rs",
        "parser/evaluate.rs",
        "tdd/hooks.rs",
        "tdd/hooks/before.rs",
        "tdd/hooks/after.rs",
        "tdd_small/hooks.rs",
        "tdd_small/hooks/before.rs",
        "tdd_small/hooks/after.rs",
    ];

    // When each existing one is measured
    let over: Vec<String> = produced
        .into_iter()
        .map(src)
        .filter(|path| path.exists())
        .map(|path| (production_lines(&path), path))
        .filter(|(lines, _)| *lines > BUDGET)
        .map(|(lines, path)| {
            format!(
                "{} at {lines} production lines",
                path.file_name().expect("a file name").to_string_lossy()
            )
        })
        .collect();

    // Then none is over
    assert!(
        over.is_empty(),
        "over the {BUDGET}-production-line budget: {over:?}"
    );
}

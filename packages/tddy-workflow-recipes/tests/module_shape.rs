//! The shape `#carve` 2/10 delivers: one module per parser phase, and the TDD hooks split by
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

/// The six phases this node gives a module each, named once so no test can drift from another.
const PHASES: [&str; 6] = [
    "planning",
    "acceptance_tests",
    "analyze",
    "green",
    "red",
    "evaluate",
];

fn parser_parent_source() -> String {
    let parent = src("parser.rs");
    std::fs::read_to_string(&parent)
        .unwrap_or_else(|error| panic!("reading {}: {error}", parent.display()))
}

/// AC1 — each of the six parser phases is its own module.
///
/// Nothing couples them: each owns its output struct, its private `Structured…` mirror, its `…De`
/// deserialization mirrors and one `parse_*_response`. They share a file because they were written
/// one after another, and the only symbol crossing every seam is `ParseError`.
#[test]
fn each_parser_phase_is_its_own_module() {
    // Given the six phases this node gives a module each
    let phases = PHASES;

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

/// AC1 — `parser.rs` keeps the error every phase shares, publishes all six, and defines none.
///
/// The facade is what keeps all nine external `tddy_workflow_recipes::parser::…` reference sites
/// resolving, so not one of them is edited. `ParseError` itself belongs to `tddy-core`; what the
/// parent keeps is the *binding*, which its own remaining parsers (`validate`, `demo`, `refactor`,
/// `update-docs`) still need.
#[test]
fn the_parser_parent_keeps_only_the_shared_error_and_a_facade() {
    // Given the parent after the split
    let parent = parser_parent_source();

    // Then it still binds the error every phase returns
    assert!(
        parent.contains("ParseError"),
        "every phase returns `ParseError`, so the parent must still bind it"
    );

    // When each phase is looked for as a published module rather than a definition
    let unpublished: Vec<&str> = PHASES
        .into_iter()
        .filter(|phase| !parent.contains(&format!("pub use {phase}::*;")))
        .collect();
    let still_defined: Vec<&str> = PHASES
        .into_iter()
        .filter(|phase| parent.contains(&format!("pub fn parse_{phase}_response")))
        .collect();

    // Then every one is published
    assert!(
        unpublished.is_empty(),
        "the parent declares no facade for these phases, so existing `parser::` paths would stop \
         resolving: {unpublished:?}"
    );

    // And none is still defined in the parent
    assert!(
        still_defined.is_empty(),
        "the parent still defines these phase parsers instead of publishing them: {still_defined:?}"
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

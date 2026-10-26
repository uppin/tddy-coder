//! The shape `#carve` 2/10 delivers: one module per parser phase, and the TDD hooks split by
//! lifecycle half.
//!
//! These assertions are the node's contract. They read the tree rather than the type system
//! deliberately — a module that exists but is never named would satisfy a compile-time check, and
//! what this node promises is a *layout* a reader can navigate.

use std::path::{Path, PathBuf};

fn src(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(relative)
}

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

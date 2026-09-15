//! What `#carve` 8/9 delivers: the `Presenter` impl partitioned along the state boundaries
//! `#carve` 4/9 created.
//!
//! The constraint that shapes this node is an operation limit, not a taste preference.
//! `extract_module` records three geometries and refuses exactly one: *"one member is lifted out of
//! an `impl` while a sibling in that same `impl` calls it"*. This file is that case unambiguously —
//! 46 methods in one `impl`, calling each other freely (`poll_workflow` calls `broadcast` and
//! `log_activity`; `handle_intent` calls `collect_answers`) — and the refusal fires **before the
//! assist runs**, with no ordering that fixes it: an `impl` body cannot hold a `mod`.
//!
//! The schema's own remedy is what this node does: *"Grow the seam to carry the whole `impl`."* Rust
//! permits a type's inherent methods across several `impl` blocks in one crate, so partitioning by
//! hand converts a refused operation into six instances of **the cheapest move Rust has** — moving a
//! whole `impl` is free of caller churn, because a method is reached through its type.

use std::path::{Path, PathBuf};

fn src(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(relative)
}

fn production(text: &str) -> String {
    match text
        .lines()
        .position(|line| line.trim_start().starts_with("#[cfg(test)]"))
    {
        Some(at) => text.lines().take(at).collect::<Vec<_>>().join("\n"),
        None => text.to_string(),
    }
}

/// The six modules the partition produces, named for the state each one serves.
const PARTITION: [&str; 6] = [
    "wiring",
    "view_channels",
    "activity",
    "questions",
    "backend_selection",
    "workflow_run",
];

/// AC1 — six modules, each holding one `impl Presenter` block.
#[test]
fn each_state_group_has_its_own_impl_block() {
    // When each module is looked for
    let absent: Vec<&str> = PARTITION
        .into_iter()
        .filter(|module| !src(&format!("presenter/presenter_impl/{module}.rs")).exists())
        .collect();

    // Then none is missing
    assert!(
        absent.is_empty(),
        "these method groups are not yet their own modules: {absent:?}"
    );
}

/// AC1 — and each really does carry an `impl`, rather than free functions that lost their receiver.
///
/// A partition that turned methods into free functions would satisfy a file-existence check and
/// change every call site in the crate, which is the opposite of what makes this move cheap.
#[test]
fn every_partition_module_carries_an_inherent_impl() {
    // Given every module the partition must produce
    let without: Vec<&str> = PARTITION
        .into_iter()
        .filter(|module| {
            let path = src(&format!("presenter/presenter_impl/{module}.rs"));
            !std::fs::read_to_string(&path)
                .unwrap_or_default()
                .contains("impl Presenter")
        })
        .collect();

    // Then each carries an `impl Presenter`
    assert!(
        without.is_empty(),
        "these modules hold no `impl Presenter`, so the methods lost their receiver: {without:?}"
    );
}

/// AC2 — the parent keeps the struct, its three state accessors and the module declarations.
#[test]
fn the_parent_keeps_only_the_struct_and_its_accessors() {
    // Given the parent after the partition
    let text =
        std::fs::read_to_string(src("presenter/presenter_impl.rs")).expect("presenter_impl.rs");
    let lines = production(&text).lines().count();

    // Then it is small
    assert!(
        lines < 250,
        "`presenter_impl.rs` is still {lines} production lines, so the partition did not happen"
    );

    // And it still declares the struct the partition is about
    assert!(
        production(&text).contains("pub struct Presenter"),
        "the struct left the parent; only its methods were meant to"
    );
}

/// AC3 — no module of the partition is over budget.
#[test]
fn no_partition_module_exceeds_five_hundred_production_lines() {
    // Given every module the partition must produce, each of which must exist to be measured
    let wrong: Vec<String> = PARTITION
        .into_iter()
        .map(|module| {
            (
                module,
                src(&format!("presenter/presenter_impl/{module}.rs")),
            )
        })
        .filter_map(|(module, path)| match std::fs::read_to_string(&path) {
            Err(_) => Some(format!("{module} does not exist")),
            Ok(text) => {
                let lines = production(&text).lines().count();
                (lines > 500).then(|| format!("{module} at {lines} production lines"))
            }
        })
        .collect();

    // Then every one exists and none is over
    assert!(
        wrong.is_empty(),
        "over the 500-production-line budget, or absent: {wrong:?}"
    );
}

/// AC5 — no method became more visible than it was.
///
/// Of the 46, most are private. A partition that made them `pub` to reach them from a sibling module
/// would have widened the crate's surface to buy a file split, which is a bad trade nobody asked for.
#[test]
fn the_partition_widens_no_method_beyond_the_crate() {
    // Given every module the partition must produce
    let wrong: Vec<String> = PARTITION
        .into_iter()
        .map(|module| {
            (
                module,
                src(&format!("presenter/presenter_impl/{module}.rs")),
            )
        })
        .filter_map(|(module, path)| match std::fs::read_to_string(&path) {
            Err(_) => Some(format!("{module} does not exist")),
            Ok(text) => production(&text)
                .lines()
                .any(|line| line.trim_start().starts_with("pub(super) fn"))
                .then(|| format!("{module} widened a private method")),
        })
        .collect();

    // Then every one exists and none reaches for a wider visibility than the methods had
    assert!(
        wrong.is_empty(),
        "widened a private method to reach it across the partition, or absent: {wrong:?}"
    );
}

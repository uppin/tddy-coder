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

/// The 27 of the 46 methods that were private before the partition. Each must still be private
/// wherever it now lives: the partition moves them, it does not open them up.
///
/// The two hub bodies (`handle_intent`, `poll_workflow`) were split into per-group handlers, and
/// those handlers are the only methods allowed `pub(super)` — they are new, so no method's
/// visibility changed to create them.
const PRIVATE_TODAY: [&str; 27] = [
    "broadcast",
    "broadcast_mode_changed",
    "broadcast_error_recovery",
    "log_activity",
    "agent_activity_head_commit",
    "agent_activity_declared_paths",
    "capture_agent_activity",
    "flush_agent_output_buffer",
    "finalize_agent_line_in_activity_log",
    "sync_agent_partial_activity_log",
    "prd_body_for_plan_review",
    "approve_plan_from_review_or_viewer",
    "select_highlight_matches",
    "sync_select_highlight",
    "clarification_answers_ready",
    "send_clarification_answers",
    "collect_answers",
    "advance_to_next_question",
    "start_workflow_from_pending_if_any",
    "apply_deferred_backend_factory",
    "handle_backend_selection_answer",
    "handle_recipe_slash_selection_answer",
    "restart_workflow",
    "spawn_workflow",
    "changeset_read_dir",
    "finish_start_slash_structured_run_if_needed",
    "try_handle_start_slash_line",
];

/// The method a line declares with some `pub` prefix (`pub`, `pub(super)`, `pub(crate)`,
/// `pub(in …)`), if it declares one.
fn pub_declared_method(line: &str) -> Option<&str> {
    let line = line.trim_start();
    if !line.starts_with("pub") {
        return None;
    }
    let (_, after_fn) = line.split_once("fn ")?;
    after_fn.split(['(', '<']).next()
}

/// The parent and every partition module, each by the name it is reported under.
fn presenter_impl_sources() -> Vec<(&'static str, PathBuf)> {
    std::iter::once(("presenter_impl", src("presenter/presenter_impl.rs")))
        .chain(PARTITION.into_iter().map(|module| {
            (
                module,
                src(&format!("presenter/presenter_impl/{module}.rs")),
            )
        }))
        .collect()
}

/// AC5 — every method that was private before the partition is still private after it.
///
/// A partition that made them `pub` in any spelling to reach them from a sibling module would have
/// widened the surface to buy a file split, which is a bad trade nobody asked for.
#[test]
fn every_method_private_before_the_partition_stays_private() {
    // Given the parent and every module the partition must produce
    let sources = presenter_impl_sources();

    // When each is searched for a formerly private method declared with a `pub` prefix
    let wrong: Vec<String> = sources
        .into_iter()
        .flat_map(|(module, path)| match std::fs::read_to_string(&path) {
            Err(_) => vec![format!("{module} does not exist")],
            Ok(text) => production(&text)
                .lines()
                .filter_map(pub_declared_method)
                .filter(|method| PRIVATE_TODAY.contains(method))
                .map(|method| format!("{module} widened `{method}`"))
                .collect(),
        })
        .collect();

    // Then every one exists and none was widened
    assert!(
        wrong.is_empty(),
        "a formerly private method was widened to reach it across the partition, or a module is absent: {wrong:?}"
    );
}

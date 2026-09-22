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

/// The production-line budget for the parent once the partition has taken its methods.
const PARENT_BUDGET: usize = 250;

/// AC2 — the parent shrinks under [`PARENT_BUDGET`] production lines but still declares the struct.
///
/// Size alone would pass a parent that handed the struct itself to a child module; the struct
/// check is what makes the shrinkage mean "the methods left", not "everything left".
#[test]
fn the_parent_shrinks_under_budget_but_keeps_the_struct() {
    // Given the parent after the partition
    let text =
        std::fs::read_to_string(src("presenter/presenter_impl.rs")).expect("presenter_impl.rs");
    let parent = production(&text);

    // When its production lines are counted
    let lines = parent.lines().count();

    // Then it is under budget
    assert!(
        lines < PARENT_BUDGET,
        "`presenter_impl.rs` is still {lines} production lines (budget {PARENT_BUDGET}), so the partition did not happen"
    );

    // And it still declares the struct the partition is about
    assert!(
        declares_struct(&parent, "Presenter"),
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

/// Where the partition modules live, relative to `src/`.
const PARTITION_DIR: &str = "presenter/presenter_impl";

/// The production text of the parent and of **every** `.rs` file under [`PARTITION_DIR`], each by
/// the name it is reported under (`presenter_impl` for the parent, the file stem for a child),
/// children sorted by path so reports are stable.
///
/// The directory is scanned rather than [`PARTITION`] listed, so a method moved into a module the
/// partition does not name is checked all the same.
fn presenter_impl_sources() -> Vec<(String, String)> {
    let mut children: Vec<PathBuf> = std::fs::read_dir(src(PARTITION_DIR))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
                .collect()
        })
        .unwrap_or_default();
    children.sort();

    std::iter::once((
        "presenter_impl".to_string(),
        src("presenter/presenter_impl.rs"),
    ))
    .chain(children.into_iter().map(|path| {
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        (stem, path)
    }))
    .map(|(name, path)| {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("{} is unreadable: {err}", path.display()));
        (name, production(&text))
    })
    .collect()
}

/// The [`PARTITION`] modules that have no file under [`PARTITION_DIR`].
fn missing_partition_modules() -> Vec<&'static str> {
    PARTITION
        .into_iter()
        .filter(|module| !src(&format!("{PARTITION_DIR}/{module}.rs")).exists())
        .collect()
}

/// Whether `text` declares `pub struct <name>` — the whole name, so `PresenterMoved` does not count.
fn declares_struct(text: &str, name: &str) -> bool {
    let needle = format!("pub struct {name}");
    text.match_indices(&needle).any(|(at, _)| {
        !text[at + needle.len()..]
            .chars()
            .next()
            .is_some_and(|after| after.is_alphanumeric() || after == '_')
    })
}

/// Whether `text` declares a method named exactly `name`: `fn <name>` not preceded by an
/// identifier character and followed by `(` or `<`, so `fn collect_answers_all(` does not count
/// for `collect_answers`.
fn declares_method(text: &str, name: &str) -> bool {
    let needle = format!("fn {name}");
    text.match_indices(&needle).any(|(at, _)| {
        let bounded_before = text[..at]
            .chars()
            .next_back()
            .is_none_or(|before| !(before.is_alphanumeric() || before == '_'));
        let bounded_after = matches!(text[at + needle.len()..].chars().next(), Some('(' | '<'));
        bounded_before && bounded_after
    })
}

/// AC5 — every method that was private before the partition is still private after it.
///
/// A partition that made them `pub` in any spelling to reach them from a sibling module would have
/// widened the surface to buy a file split, which is a bad trade nobody asked for.
#[test]
fn every_method_private_before_the_partition_stays_private() {
    // Given the parent and every module under `presenter_impl/`, and the partition modules absent
    let sources = presenter_impl_sources();
    let absent = missing_partition_modules();

    // When each is searched for a formerly private method declared with a `pub` prefix
    let widened: Vec<String> = sources
        .iter()
        .flat_map(|(module, text)| {
            text.lines()
                .filter_map(pub_declared_method)
                .filter(|method| PRIVATE_TODAY.contains(method))
                .map(move |method| format!("{module} widened `{method}`"))
        })
        .collect();

    // Then every partition module exists
    assert!(
        absent.is_empty(),
        "these partition modules are absent, so the privacy check cannot cover them: {absent:?}"
    );

    // And no scanned file declares a `PRIVATE_TODAY` method with any `pub` prefix
    assert!(
        widened.is_empty(),
        "a formerly private method was widened to reach it across the partition: {widened:?}"
    );
}

/// AC5 — every method that was private before the partition is still declared under its name.
///
/// Without this, renaming a method and then widening it would slip past the privacy check above,
/// which only looks for the names in [`PRIVATE_TODAY`].
#[test]
fn every_method_private_before_the_partition_is_still_declared() {
    // Given the parent and every module under `presenter_impl/`
    let sources = presenter_impl_sources();

    // When each formerly private method is looked for as a `fn <name>(` or `fn <name><` declaration
    let undeclared: Vec<&str> = PRIVATE_TODAY
        .into_iter()
        .filter(|method| {
            !sources
                .iter()
                .any(|(_, text)| declares_method(text, method))
        })
        .collect();

    // Then each of the 27 is declared in at least one scanned file
    assert!(
        undeclared.is_empty(),
        "these formerly private methods are declared nowhere in `presenter_impl`, so they were renamed or removed: {undeclared:?}"
    );
}

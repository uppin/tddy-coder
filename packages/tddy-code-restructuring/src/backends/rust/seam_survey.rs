use super::SYMBOL_KIND_IMPL;

use super::is_identifier;

use super::covers;

use super::PathReached;

use serde_json::Value;

use super::seam_refusal;

use crate::Result;

use crate::edit::Position;

use crate::edit::Range;

use super::line_diff;

use crate::edit::TextEdit;

/// The difference between two versions of a file, as one edit per changed region.
///
/// One edit spanning everything between the first and last change would be simpler, and is wrong:
/// the coordinate ledger folds these edits to translate later anchors, and a position *inside* a
/// replaced span cannot be translated at all. An extraction routinely changes two distant places at
/// once — it rewrites the items it relocated and every reference to them — so a single span swallows
/// every untouched line between, and the ledger then correctly reports every anchor there as removed.
/// A plan could therefore hold only one Rust extraction, which is what forced a fresh server, and a
/// fresh server re-indexes the crate.
///
/// The common prefix and suffix are trimmed first, which bounds the search; the rest is Myers' diff,
/// the same algorithm the TypeScript sidecar runs, so the two backends cannot disagree on where a
/// hunk begins.
pub(crate) fn minimal_edits(previous: &str, current: &str) -> Vec<TextEdit> {
    let before: Vec<&str> = previous.split('\n').collect();
    let after: Vec<&str> = current.split('\n').collect();

    let mut prefix = 0;
    while prefix < before.len() && prefix < after.len() && before[prefix] == after[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < before.len() - prefix
        && suffix < after.len() - prefix
        && before[before.len() - 1 - suffix] == after[after.len() - 1 - suffix]
    {
        suffix += 1;
    }

    line_diff::changed_regions(
        &before[prefix..before.len() - suffix],
        &after[prefix..after.len() - suffix],
    )
    .into_iter()
    .map(|region| TextEdit {
        range: Range {
            start: Position {
                line: (prefix + region.from) as u32 + 1,
                col: 1,
            },
            end: Position {
                line: (prefix + region.to) as u32 + 1,
                col: 1,
            },
        },
        new_text: if region.lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", region.lines.join("\n"))
        },
    })
    .collect()
}

/// Where the references to one item sit, relative to the range about to be relocated.
#[derive(Default)]
pub(crate) struct Reach {
    /// Files other than the anchor's own that reference it.
    pub(crate) stranded_in: Vec<String>,
    /// Whether anything outside the range reaches it, in this file or another.
    pub(crate) from_outside: bool,
    /// Whether one of those references is production code: in another file, or in this file
    /// outside its own `#[cfg(test)]` module.
    pub(crate) from_production: bool,
    /// One-based lines in this same file, outside the range, where something references it.
    ///
    /// Kept separately from `from_outside` because that flag answers a visibility question and
    /// collapses this case together with a reference in another file. Only the seam survey knows
    /// whether these lines matter, and they matter for exactly one kind of item.
    pub(crate) in_file_outside_at: Vec<u32>,
}

/// What the extraction knows about one item it is about to relocate.
pub(crate) struct MovedItem {
    pub(crate) name: String,
    /// The inline modules inside the relocated range that hold it, outermost first. An item with
    /// any is reached through them, so a facade that named it flat would write a path that never
    /// resolved.
    pub(crate) within: Vec<String>,
    /// The visibility keyword as written — `pub`, `pub(crate)`, `pub(super)`, or empty for private.
    pub(crate) visibility: String,
    /// Files other than the anchor's own that reference it.
    pub(crate) stranded_in: Vec<String>,
    /// Whether anything outside the range being relocated reaches it, in this file or another.
    pub(crate) reached_from_outside: bool,
    /// Whether something outside the range reaches it from production code: from another file, or
    /// from this file outside its own `#[cfg(test)]` module. An item reached only from that module
    /// is re-exported by a `named` facade under `#[cfg(test)]`.
    #[expect(
        dead_code,
        reason = "TODO(reshape-tidy-facades): implement — `named_facade_lines` reads it"
    )]
    pub(crate) reached_from_production: bool,
    /// One-based lines, in this same file, where a sibling *inside the same `impl`* still references
    /// it after the seam moves.
    ///
    /// Distinct from `reached_from_outside`, which is true of any reference beyond the range and is
    /// only a visibility signal. This one blocks a seam that cuts a trait `impl`, whose halves cannot
    /// both be `impl`s of the trait; see [`refuse_impl_sibling_references`].
    pub(crate) referenced_in_impl_at: Vec<u32>,
}

/// Refuse an extraction whose survey found nothing in a range that declares items.
///
/// An outline that lists none of the range's declarations is the silence of a server that has not
/// answered yet, not a range that moves nothing: a facade written from it names nothing (a glob comes
/// out `pub(crate)` over `pub` items, a named facade comes out empty). `range_text` is the range's
/// original lines; a range of `impl` blocks only declares no named item and is not refused.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "TODO(reshape-tidy-facades): implement — `survey_moved_items` calls it"
    )
)]
pub(crate) fn refuse_unsurveyed_range(
    range_text: &str,
    file: &str,
    range: Range,
    moved: &[MovedItem],
) -> Result<()> {
    // TODO(reshape-tidy-facades): implement
    let _ = (range_text, file, range, moved);
    todo!("refuse_unsurveyed_range")
}

/// The one-based line spans of the inline `#[cfg(test)] mod … { … }` modules of `text`.
///
/// An out-of-line `#[cfg(test)] mod x_tests;` declaration spans no lines of this file: what it holds
/// is another file, whose references count as production.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "TODO(reshape-tidy-facades): implement — `reach_of` calls it"
    )
)]
pub(crate) fn test_module_lines(text: &str) -> Vec<std::ops::RangeInclusive<u32>> {
    // TODO(reshape-tidy-facades): implement
    let _ = text;
    todo!("test_module_lines")
}

/// Refuse an extraction that would strand a reference written in another file.
///
/// Relocating items changes the path that reaches them, and rust-analyzer rewrites no reference to
/// them. Inside this file that costs nothing — the `use` the import pass restores binds the old names
/// again — but a reference in another file has nothing to rewrite it, and only the compiler would say
/// so. One does sometimes survive regardless, where the restored `use` happens to land in a module
/// the referrer sits under; that is an accident of where the seam was cut, and not something to let a
/// restructure quietly depend on.
///
/// A facade makes the whole question moot, which is why the caller skips this when one is asked for.
pub(crate) fn refuse_stranded(items: &[MovedItem]) -> Result<()> {
    let stranded: Vec<String> = items
        .iter()
        .filter(|item| !item.stranded_in.is_empty())
        .map(|item| format!("`{}` from {}", item.name, item.stranded_in.join(", ")))
        .collect();

    if stranded.is_empty() {
        return Ok(());
    }

    Err(seam_refusal(format!(
        "the module would be reached by a different path than the items moved into it are now, \
         and rust-analyzer rewrites no reference it did not move: {}. Ask for `reexport` to leave the \
         old path resolving through the parent, or cut the seam where these references do not reach.",
        stranded.join("; ")
    )))
}

/// Every item a range would relocate, however it is reached.
///
/// The companion to [`path_reached_within`], and deliberately a second traversal rather than a flag on
/// the first. They answer different questions: that one asks which items a *module path* reaches,
/// which is what decides facades and visibility, and it is right to stop above an `impl`. This one
/// asks what physically moves, so it descends through every container — which is the only way an
/// `impl` member is seen at all.
///
/// `within` carries the enclosing containers rather than only the enclosing modules, so an item's
/// entry names the `impl` that holds it. That is what lets a later pass ask whether the seam cut
/// through an `impl` instead of around it.
pub(crate) fn items_relocated_within(symbols: &Value, range: Range) -> Vec<PathReached> {
    let mut found = Vec::new();
    collect_relocated(symbols, range, &[], &mut found);
    found
}

fn collect_relocated(
    symbols: &Value,
    range: Range,
    within: &[String],
    found: &mut Vec<PathReached>,
) {
    for symbol in symbols.as_array().into_iter().flatten() {
        let name = symbol.get("name").and_then(Value::as_str);
        let covered = covers(range, symbol.pointer("/range/start"));

        // A container whose name is not a single identifier — `impl Gauge` — is still a container, so
        // it is skipped as an item and kept as a step in the path.
        if covered {
            if let (Some(item), Some(position)) = (
                name.filter(|name| is_identifier(name)),
                symbol
                    .pointer("/selectionRange/start")
                    .or_else(|| symbol.pointer("/location/range/start")),
            ) {
                found.push(PathReached {
                    name: item.to_string(),
                    within: within.to_vec(),
                    position: position.clone(),
                });
            }
        }

        // Descend whether or not the container itself is covered. A seam that cuts into an `impl`
        // starts *below* the `impl` keyword by construction, so stopping at an uncovered container
        // would walk past every member the range actually holds — which is the only geometry this
        // traversal exists to see.
        if let Some(children) = symbol.get("children") {
            let mut inside = within.to_vec();
            inside.extend(name.map(str::to_owned));
            collect_relocated(children, range, &inside, found);
        }
    }
}

/// The `impl` blocks the range cuts through rather than around, by name as the server reports them.
///
/// An `impl` is cut through when the range reaches some of its members and not others. That is the
/// geometry the extraction cannot survive: the new module is written outside the `impl`, so the
/// members left behind reference a path that never resolved from where they sit.
///
/// Computed from the members alone, because a partially covered block is exactly one with a member
/// the range does not reach — no end position required, and none is reliably reported.
pub(crate) fn impls_cut_through(symbols: &Value, range: Range) -> Vec<String> {
    let mut cut = Vec::new();
    collect_impls_cut_through(symbols, range, &mut cut);
    cut
}

fn collect_impls_cut_through(symbols: &Value, range: Range, cut: &mut Vec<String>) {
    for symbol in symbols.as_array().into_iter().flatten() {
        let children: Vec<&Value> = symbol
            .get("children")
            .and_then(Value::as_array)
            .map(|kids| kids.iter().collect())
            .unwrap_or_default();

        if symbol.get("kind").and_then(Value::as_u64) == Some(SYMBOL_KIND_IMPL) {
            let reached = children
                .iter()
                .filter(|child| covers(range, child.pointer("/range/start")))
                .count();
            if reached > 0 && reached < children.len() {
                if let Some(name) = symbol.get("name").and_then(Value::as_str) {
                    cut.push(name.to_string());
                }
            }
        }

        if let Some(kids) = symbol.get("children") {
            collect_impls_cut_through(kids, range, cut);
        }
    }
}

#[cfg(test)]
mod seam_survey_tests;

use super::seam_refusal;

use super::is_identifier_char;

use crate::{
    backends::rust::{escaping_types, module_text, relative_visibility},
    edit::VisibilityChange,
};

use crate::Result;

use super::MovedItem;

/// The visibility the assist widens everything it relocates to.
pub(crate) const WIDENED: &str = "pub(crate) ";

/// Item keywords a declaration's name can follow.
const ITEM_KEYWORDS: [&str; 8] = [
    "fn", "struct", "enum", "mod", "const", "static", "type", "trait",
];

/// Put back the visibility an item was written with, wherever nothing outside the new module needs it
/// widened, and report every widening that has to stand.
///
/// The assist rewrites everything it relocates to `pub(crate)`. Across one real restructure that
/// widened 56 items, six of them fields kept private specifically to force mutation through a single
/// method — an invariant the compiler had been enforcing, left as a comment that then contradicted the
/// code. Nothing is narrowed here that a reference still needs, so a seam that co-locates a private
/// helper with its only caller keeps the privacy, and a seam that does not says so out loud.
///
/// Two witnesses say a reference still needs it, and the second is here because the first goes
/// stale. `MovedItem::reached_from_outside` comes from a survey of the **original** text over the
/// **requested** range, taken before the assist ran. An assist that relocates only part of that
/// range rewrites what it leaves behind to reach into the module it has just written, so references
/// the survey saw *inside* the range are outside it by the time this runs. `text` is the produced
/// parent, and it is the only witness that is not stale.
pub(crate) fn restore_visibility(
    text: &str,
    module: &str,
    items: &[MovedItem],
) -> Result<(String, Vec<VisibilityChange>)> {
    let mut source: Vec<String> = text.split('\n').map(str::to_string).collect();
    let block = module_text::module_bounds(&source, module)?;
    let mut report = Vec::new();

    // Read once, before the narrowing below rewrites any declaration — and read from outside the
    // block alone, because a `module::Item` mention inside the module's own body says nothing about
    // what the parent reaches.
    let outside = outside_the_module(&source, &block);
    // A type no path names, but a widened signature does, has to stay as widened as that signature.
    let escaping = escaping_types::kept_widened(&source, &block, &outside, module, items);

    for item in items {
        relative_visibility::rebase_field_visibilities(&mut source, &block, item);

        // The assist never narrows, so an item written `pub` has nothing to answer for.
        if item.visibility == "pub" {
            continue;
        }

        // The assist widens only what was private; a relative visibility it leaves as written, and
        // as written it means something else one module deeper.
        if relative_visibility::rebase_left_as_written(&mut source, &block, item) {
            continue;
        }

        // Only inside the module the assist just wrote. A same-named item elsewhere in the file is a
        // different item, and rewriting its visibility would be a change nobody asked for.
        let Some(index) = source[block.opened..block.closed]
            .iter()
            .position(|line| declares_at_widened_visibility(line, &item.name))
            .map(|offset| block.opened + offset)
        else {
            continue;
        };

        // One live extraction on `parser.rs` ended on the second half of this condition:
        // `struct StructuredPlan` and `fn prd_value_looks_like_md_file_path` were narrowed back to
        // private while the assist had rewritten the parent to `planning::StructuredPlan` and
        // `planning::prd_value_looks_like_md_file_path`. `E0603` at the next build, after the run
        // reported `applied 1 of 1 operations`.
        if item.reached_from_outside
            || reaches_through_module(&outside, module, &item.name)
            || escaping.contains(&item.name)
        {
            report.push(VisibilityChange {
                item: item.name.clone(),
                from: if item.visibility.is_empty() {
                    "private".to_string()
                } else {
                    item.visibility.clone()
                },
                to: "pub(crate)".to_string(),
            });
            continue;
        }

        source[index] = relative_visibility::put_back(&source[index], item);
    }

    Ok((source.join("\n"), report))
}

/// Everything in `source` that is not inside `block`, as one text.
fn outside_the_module(source: &[String], block: &module_text::ModuleBlock) -> String {
    source
        .iter()
        .enumerate()
        .filter(|(index, _)| *index < block.opened || *index > block.closed)
        .map(|(_, line)| line.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether `text` reaches `name` through `module` — `planning::StructuredPlan`.
///
/// Read as a path rather than as a substring, because the two neighbouring mistakes are both real
/// source: `myplanning::StructuredPlan` names a different module, and `planning::StructuredPlanner`
/// a different item. A qualifier in front is not one of them — `crate::planning::StructuredPlan`
/// reaches this item, and the `::` before it is not an identifier character.
pub(crate) fn reaches_through_module(text: &str, module: &str, name: &str) -> bool {
    let path = format!("{module}::{name}");
    let mut searched = 0;

    while let Some(offset) = text[searched..].find(&path) {
        let at = searched + offset;
        let before = text[..at].chars().next_back();
        let after = text[at + path.len()..].chars().next();
        if !before.is_some_and(is_identifier_char) && !after.is_some_and(is_identifier_char) {
            return true;
        }
        searched = at + path.len();
    }

    false
}

/// Whether `line` declares `name` at the visibility the assist widened it to.
pub(crate) fn declares_at_widened_visibility(line: &str, name: &str) -> bool {
    let Some(rest) = line.trim_start().strip_prefix(WIDENED) else {
        return false;
    };

    declares_item(rest, name)
}

/// Whether `line` declares `name` as an item, at whatever visibility it carries.
///
/// The visibility-agnostic form of [`declares_at_widened_visibility`]. That one answers "did the
/// assist widen this", which is the narrowing pass's question; this one answers "is this item here
/// at all", which is the relocation guard's. An item the seam moved that was already `pub` is never
/// widened, so keying the guard on the widened form would report it as left behind.
pub(crate) fn declares_item(line: &str, name: &str) -> bool {
    let tokens: Vec<&str> = line
        .split(|character: char| !is_identifier_char(character))
        .filter(|token| !token.is_empty())
        .collect();

    tokens
        .windows(2)
        .any(|pair| ITEM_KEYWORDS.contains(&pair[0]) && pair[1] == name)
}

/// Refuse an extraction whose assist relocated less than the anchor asked for.
///
/// rust-analyzer decides the extraction's real extent, and it does not have to agree with the range
/// it was handed. When it moves part of that range it rewrites the remainder **in place**, reaching
/// into the module it has just written through qualified `module::Item` paths. One live extraction
/// anchored at lines 10–152 came back having relocated 10–116, and the run reported
/// `applied 1 of 1 operations` over a parser module that had been split in half.
///
/// With visibility now decided on the produced text that result compiles, which is precisely why it
/// needs saying out loud: a silent partial split is a seam the author did not ask for, and a plan
/// whose next step assumes the whole range moved is built on it. A `#carve` node promising "one
/// module per phase" would go green here and fail its own shape assertions later.
///
/// Keyed on the surveyed items rather than on a line count. [`path_reached_within`] collects exactly
/// the path-reachable items the range covered, so an item absent from the produced module is
/// material by construction — and trailing trivia, a blank line or a comment the assist declined to
/// carry never trips it.
pub(crate) fn refuse_partial_relocation(
    text: &str,
    module: &str,
    anchored: &[MovedItem],
) -> Result<()> {
    let source: Vec<String> = text.split('\n').map(str::to_string).collect();
    let block = module_text::module_bounds(&source, module)?;
    let relocated = &source[block.opened..block.closed];

    let left_behind: Vec<&str> = anchored
        .iter()
        .filter(|item| !relocated.iter().any(|line| declares_item(line, &item.name)))
        .map(|item| item.name.as_str())
        .collect();

    if left_behind.is_empty() {
        return Ok(());
    }

    Err(seam_refusal(format!(
        "rust-analyzer relocated part of the anchored range and left {} behind: {}. It rewrote what \
         stayed to reach into `{module}`, so the seam is split rather than extracted — the module \
         the plan described does not exist. Anchor the range with `restructure anchors --items`, \
         which covers whole items and their trivia, or cut the seam where the assist will carry all \
         of it.",
        if left_behind.len() == 1 { "an item" } else { "items" },
        left_behind.join(", ")
    )))
}

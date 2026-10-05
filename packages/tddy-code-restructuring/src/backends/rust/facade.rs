use super::seam_refusal;

use crate::{
    backends::rust::{module_text, seam_survey},
    plan::Reexport,
};

use super::server_defect;

use crate::Result;

use crate::edit::VisibilityChange;

/// The visibilities the assist widened on members no path-reached survey ever sees.
///
/// `restore_visibility` iterates the items [`path_reached_within`] returned, and that traversal stops
/// above an `impl` — so a relocated `impl` member is neither put back nor named, and lands
/// `pub(crate)` in silence while the schema promises every widening is reported.
///
/// Reported rather than narrowed, deliberately. Narrowing would reintroduce `E0624` for a private
/// method with a sibling-module caller, which is safe today precisely *because* the item stays
/// widened; naming it makes that a decision instead of an accident.
///
/// A member written `pub` or already `pub(crate)` has nothing to answer for, so the visibility each
/// one was written with is compared rather than inferred from the relocated text — which cannot tell a
/// widening from an item that always read that way.
pub(crate) fn impl_widenings(
    source: &[String],
    block: &module_text::ModuleBlock,
    members: &[seam_survey::MovedItem],
) -> Vec<VisibilityChange> {
    members
        .iter()
        .filter(|member| {
            member.visibility != "pub" && member.visibility != super::visibility::WIDENED.trim()
        })
        .filter(|member| {
            source[block.opened..block.closed]
                .iter()
                .any(|line| super::visibility::declares_at_widened_visibility(line, &member.name))
        })
        .map(|member| VisibilityChange {
            item: member.name.clone(),
            from: if member.visibility.is_empty() {
                "private".to_string()
            } else {
                member.visibility.clone()
            },
            to: super::visibility::WIDENED.trim().to_string(),
        })
        .collect()
}

/// The `use` lines the parent keeps so paths that reached the relocated items still resolve.
///
/// A glob is one line and legal whatever moved: a glob re-export caps at each item's own visibility
/// rather than failing on a member less visible than itself. A named re-export cannot do that — `pub
/// use` of a `pub(crate)` item is `E0365` — so its names are grouped by the visibility each item was
/// written with, widest first. It names only the items something outside the new module reaches,
/// because re-exporting a helper that travelled with its only caller would make it reachable for
/// nobody and undo the privacy the seam just preserved.
/// Refuse a rewrite that produced a qualified path naming something the seam never moved.
///
/// One real run wrote `seeded_clone_guard::SeededCloneGuardloneGuardloneGuard` — the identifier's
/// tail inserted twice at a four-character offset, which is two edits against the same reference
/// with the second computed on pre-first-edit text. It appeared once among ~20 rewrites of that
/// symbol and the run still reported success.
///
/// The existing residual-placeholder check cannot see this: it counts occurrences of the
/// *placeholder* (`modname`, `fun_name`), not of the references the assist rewrote. This one is
/// keyed to what the seam moved, which the backend already knows, and costs a single pass over the
/// produced text with no server round trip.
///
/// Only paths whose qualifier is this module are weighed. A path through any other module was not
/// written by this operation.
pub(crate) fn refuse_mangled_rewrite(
    text: &str,
    module: &str,
    moved: &[seam_survey::MovedItem],
) -> Result<()> {
    let known: Vec<&str> = moved.iter().map(|item| item.name.as_str()).collect();
    let needle = format!("{module}::");

    for (index, line) in text.split('\n').enumerate() {
        let mut rest = line;
        while let Some(at) = rest.find(&needle) {
            // A longer qualifier ending in this module's name is a different module.
            let boundary_ok = rest[..at]
                .chars()
                .last()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_' && c != ':');
            rest = &rest[at + needle.len()..];
            if !boundary_ok {
                continue;
            }
            let ident: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            // Nested modules and associated items are reached through further segments, and a
            // lower-case head is a module or function rather than a moved type.
            if ident.is_empty() || known.contains(&ident.as_str()) {
                continue;
            }
            if let Some(base) = known.iter().find(|name| ident.starts_with(*name)) {
                return Err(server_defect(format!(
                    "the rewrite of `{base}` produced `{module}::{ident}` on line {} — the \
                     identifier was written over itself, which is a corrupted edit rather than a \
                     path. Refusing rather than reporting success over source that will not build.",
                    index + 1
                )));
            }
        }
    }

    Ok(())
}

/// The widest visibility any relocated item carries, as the keyword a facade should re-export at.
///
/// Defaults to `pub(crate)` rather than `pub`: a seam that moved nothing public has nothing to
/// publish, and `pub(crate)` is both what the assist widened its members to and the visibility the
/// parent's own dependents reach the facade through.
fn widest_visibility(items: &[seam_survey::MovedItem]) -> &'static str {
    if items.iter().any(|item| item.visibility == "pub") {
        "pub"
    } else {
        "pub(crate)"
    }
}

/// Whether the facade this seam is getting will itself bind `name` in the parent's scope.
///
/// Mirrors [`facade_lines`]: a glob re-exports everything the module holds, while a named facade
/// covers only the top-level items something outside the seam reaches.
pub(crate) fn facade_will_bind(
    name: &str,
    moved: &[seam_survey::MovedItem],
    kind: Reexport,
) -> bool {
    match kind {
        // `outside` is refused for `extract_module` when the plan is read, so it never gets here.
        Reexport::None | Reexport::Outside => false,
        Reexport::Glob => moved.iter().any(|item| item.name == name),
        Reexport::Named => moved
            .iter()
            .any(|item| item.name == name && item.reached_from_outside && item.within.is_empty()),
    }
}

pub(crate) fn facade_lines(
    module: &str,
    items: &[seam_survey::MovedItem],
    kind: Reexport,
) -> Result<Vec<String>> {
    Ok(match kind {
        Reexport::Outside => {
            return Err(super::failure(
                "`reexport: outside` belongs to `move_item` and `reparent_module`; an extraction \
                 leaves a `glob` or `named` facade"
                    .to_string(),
            ))
        }
        Reexport::None => Vec::new(),
        // `pub use` only where something the module holds is actually `pub`. The assist rewrites
        // what it relocates to `pub(crate)`, so a seam of private items yields a `pub` glob that
        // re-exports nothing — `clippy::unused_imports` calls that out by name, and under
        // `-D warnings` it fails the build the restructure was supposed to leave green.
        Reexport::Glob => vec![format!("{} use {module}::*;", widest_visibility(items))],
        Reexport::Named => {
            refuse_uncovered_nesting(items)?;
            let reached: Vec<&seam_survey::MovedItem> = items
                .iter()
                .filter(|item| item.reached_from_outside && item.within.is_empty())
                .collect();

            let mut tiers: Vec<&str> = Vec::new();
            for item in &reached {
                if !tiers.contains(&item.visibility.as_str()) {
                    tiers.push(item.visibility.as_str());
                }
            }
            // Widest first, and stable within a tier so the order follows the source.
            tiers.sort_by_key(|tier| match *tier {
                "pub" => 0,
                "" => 2,
                _ => 1,
            });

            tiers
                .into_iter()
                .map(|tier| {
                    let names: Vec<&str> = reached
                        .iter()
                        .filter(|item| item.visibility == tier)
                        .map(|item| item.name.as_str())
                        .collect();
                    let prefix = if tier.is_empty() {
                        String::new()
                    } else {
                        format!("{tier} ")
                    };
                    format!("{prefix}use {module}::{{{}}};", names.join(", "))
                })
                .collect()
        }
    })
}

/// Refuse a named facade for an item whose own module the facade will not carry.
///
/// A nested item is reached through the module holding it, so re-exporting that module keeps
/// `parent::nested::buried` resolving untouched and naming the item as well would only publish
/// `parent::buried` — a path no caller ever used. Naming it flat is worse still: that is the
/// `pub use grouped::{nested, buried};` that earned `E0432` while the run reported success.
///
/// The residual case has no honest line at all: something outside reaches the nested item, nothing
/// reaches the module holding it, so the facade would have to invent a path or drop the reference.
/// Refuse, and say which item and which module, because the fix is a different seam.
fn refuse_uncovered_nesting(items: &[seam_survey::MovedItem]) -> Result<()> {
    let carried: Vec<&str> = items
        .iter()
        .filter(|item| item.reached_from_outside && item.within.is_empty())
        .map(|item| item.name.as_str())
        .collect();

    let uncovered: Vec<String> = items
        .iter()
        .filter(|item| item.reached_from_outside)
        .filter_map(|item| {
            let holder = item.within.first()?;
            (!carried.contains(&holder.as_str()))
                .then(|| format!("`{}` inside `{holder}`", item.name))
        })
        .collect();

    if uncovered.is_empty() {
        return Ok(());
    }

    Err(seam_refusal(format!(
        "a named re-export cannot keep these paths resolving: {}. Nothing outside reaches the module \
         holding them, so there is no name the parent could re-export that would cover them. Ask for \
         `reexport: glob`, which re-exports the module too, or cut the seam so the nested item stays \
         behind.",
        uncovered.join("; ")
    )))
}

/// What a `named` re-export that re-exported nothing has to say for itself.
///
/// A seam whose items are all internal legitimately needs no facade, so this is not a refusal — but
/// a developer who asked for a facade and got none has to hear it here rather than find it in the
/// diff.
pub(crate) fn empty_facade_note(module: &str, lines: &[String], kind: Reexport) -> Option<String> {
    (kind == Reexport::Named && lines.is_empty()).then(|| {
        format!(
            "`reexport: named` on `{module}` re-exported nothing: everything the seam moved is \
             reached only from inside it. The facade was asked for and is not there."
        )
    })
}

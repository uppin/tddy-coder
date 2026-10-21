//! What a member move checks before it writes anything, and what the shared member-run reader does
//! not check for it.
//!
//! S4 is a post-read check: the reader serves `retarget_impl` as well, and a macro invocation between
//! two members is a refusal only a member move makes. S5 needs the destination's text, which the
//! reader never sees.

use std::collections::BTreeSet;
use std::ops::Range;

use super::super::crate_move::source_scan::items_of_module;
use super::super::item_move::outline::Item;
use super::super::retarget_impl::outline::Run;
use super::super::seam_refusal;
use crate::Result;

/// S4: the lines from a run's first member to its last hold nothing but blank lines and comments
/// between the members. A macro invocation, or any item the outline does not list, cannot move with
/// the run, so it refuses the run rather than leave it behind.
pub(super) fn refuse_a_non_member(text: &str, run: &Run) -> Result<()> {
    let (Some(first), Some(last)) = (run.moved().first(), run.moved().last()) else {
        return Ok(());
    };
    for line in first.first_line..=last.last_line {
        let covered = run
            .members
            .iter()
            .any(|member| (member.first_line..=member.last_line).contains(&line));
        let code = text
            .lines()
            .nth(line as usize - 1)
            .unwrap_or_default()
            .trim();
        let trivia = code.is_empty() || code.starts_with("//") || code.starts_with('*');
        if !covered && !trivia {
            return Err(seam_refusal(format!(
                "line {line} is `{code}`, which is not a member of the `impl`: it would move with the run"
            )));
        }
    }
    Ok(())
}

/// S5: a name the moved code reaches in the origin that the destination binds to a different item:
/// one it declares, or one it imports by a path other than the origin's own `crate::<origin>::<name>`.
/// The moved code would call the destination's item, so the move is refused rather than written.
pub(super) fn refuse_a_name_bound_elsewhere(
    destination_text: &str,
    scope: &Range<usize>,
    origin: &[String],
    reached: &[Item],
) -> Result<()> {
    let items = items_of_module(&destination_text[scope.clone()]);
    let declared: BTreeSet<&str> = items
        .defined
        .iter()
        .map(String::as_str)
        .chain(items.children.iter().map(|child| child.name.as_str()))
        .collect();
    for item in reached {
        let name = item.name.as_str();
        let mut origin_path = vec!["crate".to_string()];
        origin_path.extend(origin.iter().cloned());
        origin_path.push(name.to_string());
        let imported_elsewhere = items
            .uses
            .iter()
            .filter(|leaf| leaf.bound_name() == Some(name))
            .any(|leaf| leaf.segments != origin_path);
        if declared.contains(name) || imported_elsewhere {
            return Err(seam_refusal(format!(
                "the moved code names `{name}`, which the destination binds to another item: the \
                 moved members would call that one, not `{}`",
                origin_path.join("::")
            )));
        }
    }
    Ok(())
}

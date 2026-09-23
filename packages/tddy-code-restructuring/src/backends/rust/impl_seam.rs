//! Seams that cut an `impl` in half: which ones can move, and the one repair the assist needs.
//!
//! Split out of `backends/rust.rs`, which is past its size budget.

use super::{declares, is_identifier_char, seam_refusal, Block, MovedItem};
use crate::Result;

/// Refuse a seam that cuts a trait `impl` in half while a member left behind still calls one that
/// moves.
///
/// Three geometries look alike, and only the last blocks:
///
/// - A whole `impl` moves while the parent calls its methods: a method is reached through its type,
///   so there is nothing to rewrite.
/// - Some members of an **inherent** `impl` move while one left behind calls them. The assist writes
///   them as `mod … { use super::Gauge; impl Gauge { … } }`, so they stay methods of the same type
///   and `self.doubled()` resolves from either side of the cut — once the assist's rewrite of that
///   call is undone by [`with_method_calls_restored`]. A private member is widened to
///   `pub(crate)` by the assist, which [`super::impl_widenings`] reports and nothing narrows back.
/// - Some members of a **trait** `impl` move. The new module would hold a second `impl Meter for
///   Gauge` (`E0119`) and each half would lack the other's items (`E0046`). No rewrite repairs that.
///
/// A member whose enclosing `impl` is not known is refused as the third case: this cannot tell it is
/// safe, and a refusal costs less than source that does not compile.
///
/// The prescription differs from [`super::refuse_residual_placeholder`]'s deliberately. Reordering is the fix
/// when the stranded reference sits in an already-extracted *module*; it cannot help here, because an
/// `impl` body cannot hold a `mod`. The seam has to grow.
pub(super) fn refuse_impl_sibling_references(items: &[MovedItem]) -> Result<()> {
    let blocked: Vec<String> = items
        .iter()
        .filter(|item| !item.referenced_in_impl_at.is_empty())
        .filter(|item| {
            !item
                .within
                .last()
                .is_some_and(|holder| is_inherent_impl(holder))
        })
        .map(|item| {
            let lines = item
                .referenced_in_impl_at
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            match item.within.last() {
                Some(holder) => format!("`{}` of `{holder}` from line(s) {lines}", item.name),
                None => format!("`{}` from line(s) {lines}", item.name),
            }
        })
        .collect();

    if blocked.is_empty() {
        return Ok(());
    }

    Err(seam_refusal(format!(
        "this seam cuts an `impl` in half, and a member left behind still calls one that would move: \
         {}. Half of a trait `impl` cannot move: the new module would hold a second `impl` of the same \
         trait for the same type, and each half would lack the other's items. An `impl` body cannot \
         hold a `mod`, so no ordering helps — grow the seam to carry the whole `impl`, or cut it \
         where nothing crosses.",
        blocked.join("; ")
    )))
}

/// Whether an `impl` as the outline names it — `impl Gauge`, `impl Meter for Gauge` — is inherent.
///
/// Whole tokens, so a type named `Format` is not read as `for`. A higher-ranked bound
/// (`for<'a> Fn(&'a T)`) reads as a trait `impl`, which only ever costs a refusal.
fn is_inherent_impl(holder: &str) -> bool {
    declares(holder) == Some(Block::Impl)
        && !holder
            .split(|character: char| !is_identifier_char(character))
            .any(|token| token == "for")
}

/// The assist's output with its rewrite of a moved method's calls undone.
///
/// `extract_module` prefixes every reference to an item it moves with the new module's path, and it
/// does that to a method call too: `self.doubled()` left behind comes back as
/// `self.modname::doubled()`. That is not a path and not Rust, the rename of `modname` cannot reach
/// it, and the call was right as it stood. A member of an inherent `impl` moves as a member of an
/// `impl` of the same type, so the receiver still finds it wherever the call is.
///
/// Only a `.` directly before the placeholder is undone, and only for the inherent members the seam
/// moves. A path through the new module that does not follow a `.` is the assist's to write, and is
/// left for the rename.
pub(super) fn with_method_calls_restored(
    text: &str,
    placeholder: &str,
    members: &[MovedItem],
) -> String {
    let mut restored = text.to_string();

    for member in members.iter().filter(|member| {
        member
            .within
            .last()
            .is_some_and(|holder| is_inherent_impl(holder))
    }) {
        let rewritten = format!(".{placeholder}::{}", member.name);
        let mut from = 0;
        while let Some(found) = restored[from..]
            .find(&rewritten)
            .map(|offset| from + offset)
        {
            let end = found + rewritten.len();
            let whole = !restored[end..].starts_with(is_identifier_char);
            if whole {
                restored.replace_range(found + 1..end - member.name.len(), "");
            }
            from = found + 1;
        }
    }

    restored
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A member of the `impl` the outline names `holder`.
    fn a_member_of(holder: &str, name: &str) -> MovedItem {
        MovedItem {
            name: name.to_string(),
            within: vec![holder.to_string()],
            visibility: String::new(),
            stranded_in: Vec::new(),
            reached_from_outside: true,
            referenced_in_impl_at: vec![9],
        }
    }

    #[test]
    fn puts_back_a_method_call_the_assist_wrote_as_a_path() {
        // Given
        let produced = "        self.modname::doubled() + 1\n";

        // When
        let restored = with_method_calls_restored(
            produced,
            "modname",
            &[a_member_of("impl Gauge", "doubled")],
        );

        // Then
        assert_eq!(restored, "        self.doubled() + 1\n");
    }

    #[test]
    fn leaves_a_path_through_the_module_that_follows_no_receiver() {
        // Given
        let produced = "    modname::doubled(gauge)\n";

        // When
        let restored = with_method_calls_restored(
            produced,
            "modname",
            &[a_member_of("impl Gauge", "doubled")],
        );

        // Then
        assert_eq!(restored, produced);
    }

    #[test]
    fn leaves_a_longer_name_the_moved_one_is_only_a_prefix_of() {
        // Given
        let produced = "        self.modname::doubled_twice()\n";

        // When
        let restored = with_method_calls_restored(
            produced,
            "modname",
            &[a_member_of("impl Gauge", "doubled")],
        );

        // Then
        assert_eq!(restored, produced);
    }

    /// A trait `impl` cut is refused before the assist runs; this repair is not what makes one pass.
    #[test]
    fn leaves_the_call_to_a_trait_member_alone() {
        // Given
        let produced = "        self.modname::doubled() + 1\n";

        // When
        let restored = with_method_calls_restored(
            produced,
            "modname",
            &[a_member_of("impl Meter for Gauge", "doubled")],
        );

        // Then
        assert_eq!(restored, produced);
    }
}

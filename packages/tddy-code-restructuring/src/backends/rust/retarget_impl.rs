//! `retarget_impl`: moving members of an inherent `impl` to another type of the same crate.
//!
//! The whole block's self type changes, or the block is split at the anchored run of members. The
//! members, their attributes and comments are byte ranges of the source; the only text authored here
//! is the header's self type, the repeated `impl` headers and the one `use` of the new type. See
//! `docs/dev/1-WIP/2026-10-05-sharpen-retarget-impl.md` for the rules (the splitting rule, the path
//! re-points, the field refusal).
//!
//! TODO(retarget-impl): the operation is published and not implemented. Until it is, `check` says
//! so as a finding and `resolve` refuses naming the node, so nothing believes a retarget happened.

mod unfinished;

use super::RustBackend;
use crate::edit::Resolution;
use crate::plan::RefactorOp;
use crate::registry::Workspace;
use crate::Result;

/// What a static check finds wrong with a `retarget_impl`, from the plan and the text alone.
///
/// TODO(retarget-impl): implement P7 (the new type is in another package) and P8 (its module
/// declares no such type), which need no server.
pub(super) fn findings(_op: &RefactorOp, _workspace: &Workspace<'_>) -> Result<Vec<String>> {
    Ok(vec![unfinished::reason()])
}

impl RustBackend {
    /// Resolve a `retarget_impl` into the edit to the one file that holds the block.
    ///
    /// TODO(retarget-impl): implement (outline reading S1-S3, field refusal S4/S5, the header
    /// rewrite and split, the path re-points, the `use` and S6).
    pub(super) fn retarget_impl(
        &mut self,
        _op: &RefactorOp,
        _workspace: &Workspace<'_>,
    ) -> Result<Resolution> {
        Err(unfinished::refusal())
    }
}

#[cfg(test)]
mod tests {
    use super::super::seam_survey::minimal_edits;

    const BEFORE: &str = "impl Host {\n    fn get(&self) {}\n\n    fn put(&mut self) {}\n    fn last(&self) {}\n\n    fn size(&self) {}\n}\n";
    const AFTER: &str = "impl Host {\n    fn get(&self) {}\n}\n\nimpl Roster {\n    fn put(&mut self) {}\n    fn last(&self) {}\n}\n\nimpl Host {\n    fn size(&self) {}\n}\n";

    /// M0, third premise: a split is small insertions at the two cuts (a closing brace and the next
    /// header at each), never one hunk over the whole block, so the members that stay are untouched
    /// and the ledger can translate every anchor inside them.
    #[test]
    fn splitting_a_block_gives_insertions_at_the_two_cuts_and_leaves_every_member_alone() {
        // Given a block of four members and the same block split around its middle two
        let (before, after) = (BEFORE, AFTER);

        // When the difference is taken
        let hunks = minimal_edits(before, after);

        // Then it is four insertions, two at each cut, and they are header text only
        let texts: Vec<&str> = hunks.iter().map(|hunk| hunk.new_text.as_str()).collect();
        assert_eq!(texts, ["}\n", "impl Roster {\n", "}\n", "impl Host {\n"]);
        assert!(
            hunks.iter().all(|hunk| hunk.range.start == hunk.range.end),
            "a hunk replaced text a member owns: {hunks:?}"
        );
    }
}

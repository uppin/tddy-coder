//! `retarget_impl`: moving members of an inherent `impl` to another type of the same crate.
//!
//! The whole block's self type changes, or the block is split at the anchored run of members. The
//! members, their attributes and comments are byte ranges of the source; the only text authored here
//! is the header's self type, the repeated `impl` headers and the one `use` of the new type. See
//! `docs/dev/1-WIP/2026-10-05-sharpen-retarget-impl.md` for the rules (the splitting rule, the path
//! re-points, the field refusal).

mod delegator;
mod fields;
mod imports;
mod outline;
mod preflight;
mod rewrite;

use serde_json::Value;

use super::item_move::sites::Site;
use super::item_move::text::{applied, line_start, use_insertion, Edit};
use super::{failure, seam_refusal, seam_survey, uri_of, RustBackend};
use crate::edit::{FileEdit, Range, Resolution, WorkspaceEdit};
use crate::item_anchor::{span_of, unlowered_item_anchor};
use crate::plan::{Anchor, RefactorOp};
use crate::registry::Workspace;
use crate::Result;

/// What a static check finds wrong with a `retarget_impl`, from the plan and the text alone.
///
/// P7 (the new type is in another package) and P8 (its module declares no such type) need no
/// server, so a plain `check` reports them without paying for an index.
pub(super) fn findings(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>> {
    let mut findings = preflight::findings(op, workspace)?;
    findings.extend(deferred_delegator(op));
    Ok(findings)
}

/// The delegator milestone is deferred, so a plan that asks for one is refused rather than
/// retargeted without the forwarding method it asked for.
///
/// TODO(retarget-impl): the `leave_delegator` variant and `expr` are the delegator's; the follow-up
/// node implements the emitter (P9, S7, R3) and this refusal goes with it. Until then the schema
/// still names the fields, so a plan that carries them must not read as honoured.
fn deferred_delegator(op: &RefactorOp) -> Option<String> {
    (op.variant.is_some() || op.expr.is_some()).then(|| {
        "`retarget_impl`'s forwarding delegator (`variant: \"leave_delegator\"` with `expr`) is \
         not implemented yet (node `retarget-impl`, deferred to a follow-up), so the members would \
         move without the forwarding method the plan asked for"
            .to_string()
    })
}

/// The guard the server never sees: the static findings re-run, the deferred-delegator refusal, and
/// the `to_type` the plan must name — as the type to retarget, its new self text and its bare name.
fn the_retarget<'a>(
    op: &'a RefactorOp,
    workspace: &Workspace<'_>,
) -> Result<(&'a str, &'a str, &'a str)> {
    // The static findings are re-run here, before any server, so `apply` refuses a plan a plain
    // `check` would have reported — the parity every static preflight in this backend holds.
    if let Some(finding) = preflight::findings(op, workspace)?.into_iter().next() {
        return Err(failure(finding));
    }
    if let Some(reason) = deferred_delegator(op) {
        return Err(crate::RestructureError::UnsupportedOp {
            backend: reason,
            op: "RetargetImpl".to_string(),
        });
    }
    let to_type = op.to_type.as_deref().ok_or_else(|| {
        crate::RestructureError::MalformedPlan(
            "`retarget_impl` needs `to_type`: the type the members move to".to_string(),
        )
    })?;
    let new_self = to_type.rsplit("::").next().unwrap_or(to_type);
    let new_name = base_name(new_self);
    Ok((to_type, new_self, new_name))
}

impl RustBackend {
    /// Resolve a `retarget_impl` into the edit to the one file that holds the block.
    pub(super) fn retarget_impl(
        &mut self,
        op: &RefactorOp,
        workspace: &Workspace<'_>,
    ) -> Result<Resolution> {
        let file = op.anchor.file();
        let text = workspace.read(file)?;
        let (to_type, new_self, new_name) = the_retarget(op, workspace)?;
        let range = self.retarget_range(op, workspace)?;

        let uri = uri_of(&workspace.root.join(file));
        self.start(workspace.root)?;
        self.did_open(&uri, &text)?;
        self.ensure_indexed(&uri)?;
        let symbols = self.settled_outline(&uri)?;

        let run = outline::read(&symbols, &text, range)?;
        if run.self_type == new_name {
            return Err(seam_refusal(format!(
                "`{}` is already the self type of this `impl`: there is nothing to retarget",
                run.self_type
            )));
        }

        // The field check is the first thing after the outline and before any edit is built, so a
        // `check --deep` names it and an `apply` refuses with the tree untouched.
        let moved: Vec<(&str, &str)> = run
            .moved()
            .iter()
            .map(|member| (member.name.as_str(), member_text(&text, member)))
            .collect();
        let declaration = fields::declaration(workspace, file, to_type)?;
        fields::refuse(&moved, new_name, &declaration, file)?;

        let layout = rewrite::Layout::of(&text, &run, new_self);

        let named: Vec<(&str, &Value)> = run
            .moved()
            .iter()
            .map(|member| (member.name.as_str(), &member.position))
            .collect();
        let sites = self.sites_of(&uri, workspace, file, &text, &named)?;
        let retarget = Retarget {
            op,
            text: &text,
            file,
            new_name,
        };
        let (replacement, notes) = the_replacement(&retarget, &layout, &run, &sites)?;

        let use_line = imports::the_use(workspace, file, to_type)?;
        let edits = the_edits(&text, replacement, layout.replaced.clone(), use_line);
        let new_text = applied(&text, &edits)?;

        Ok(Resolution {
            edit: WorkspaceEdit {
                changes: vec![FileEdit::Change {
                    path: file.to_string(),
                    edits: seam_survey::minimal_edits(&text, &new_text),
                }],
            },
            report: Vec::new(),
            notes,
        })
    }

    /// The lines the anchor covers: as written when a run lowered it already, otherwise resolved the
    /// way an item anchor would.
    fn retarget_range(&mut self, op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Range> {
        match &op.anchor {
            Anchor::Range { start, end, .. } => Ok(Range {
                start: *start,
                end: *end,
            }),
            Anchor::Item { .. } | Anchor::Items { .. } => span_of(&op.anchor, workspace.root, self),
            Anchor::Symbol { .. } => Err(unlowered_item_anchor(&op.anchor, "`retarget_impl`")),
        }
    }
}

/// What [`the_replacement`] reads of the run besides the layout: the operation, the file's text and
/// path, and the new type's bare name.
struct Retarget<'a> {
    op: &'a RefactorOp,
    text: &'a str,
    file: &'a str,
    new_name: &'a str,
}

/// The text that replaces the block, with the `Old::` paths inside the moved members re-pointed,
/// and the notes the run reports: with `variant: "leave_delegator"`, a delegator in each moved
/// member's slot (S7 refused first) and a note for each delegator nothing calls.
fn the_replacement(
    retarget: &Retarget<'_>,
    layout: &rewrite::Layout<'_>,
    run: &outline::Run,
    sites: &[Site],
) -> Result<(String, Vec<String>)> {
    let Retarget {
        op,
        text,
        file,
        new_name,
    } = retarget;
    let inside: Vec<(usize, String)> = sites
        .iter()
        .filter(|site| site.path == *file && layout.moved.contains(&site.offset))
        .map(|site| (site.offset - layout.moved.start, site.name.clone()))
        .collect();
    let moved_text = rewrite::repointed(
        &text[layout.moved.clone()],
        &inside,
        &run.self_type,
        new_name,
    );
    let Some(expr) = op.expr.as_deref().filter(|_| op.variant.is_some()) else {
        return Ok((layout.assemble(&moved_text), Vec::new()));
    };

    let members: Vec<(&str, &str)> = run
        .moved()
        .iter()
        .map(|member| (member.name.as_str(), member_text(text, member)))
        .collect();
    delegator::refuse_unforwardable(&members)?;
    let delegators = members
        .iter()
        .map(|(_, member)| delegator::forwarding_method(member, expr, new_name))
        .collect::<Result<Vec<String>>>()?;
    let names: Vec<&str> = members.iter().map(|(name, _)| *name).collect();
    let notes =
        delegator::dead_delegators(sites, file, layout.moved.clone(), &names, &run.self_type);
    Ok((layout.with_delegators(&moved_text, &delegators), notes))
}

/// The edits that move the block: the replacement of the anchored range, plus the `use` of the new
/// type, inserted at the top, when the new type needs one.
fn the_edits(
    text: &str,
    replacement: String,
    replaced: std::ops::Range<usize>,
    use_line: Option<String>,
) -> Vec<Edit> {
    let mut edits = vec![Edit::replace(replaced, replacement)];
    if let Some(line) = use_line {
        let (at, blank) = use_insertion(text, 0..text.len());
        let mut inserted = format!("{line}\n");
        if blank {
            inserted.push('\n');
        }
        edits.push(Edit::insert(at, inserted));
    }
    edits
}

/// The text of one member, its attached trivia included.
fn member_text<'a>(text: &'a str, member: &outline::Member) -> &'a str {
    let start = line_start(text, member.first_line);
    let end = line_start(text, member.last_line + 1);
    &text[start..end]
}

/// A type as its bare name: no generic arguments.
fn base_name(written: &str) -> &str {
    written.split('<').next().unwrap_or(written).trim()
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

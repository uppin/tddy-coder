//! The survey of a member move: what the moved lines name in the origin (R4), and the widenings the
//! split needs (R6).
//!
//! The server answers the outline of each file and who names each member or item
//! (`textDocument/references`). The widenings are then a function of those answers and of the texts,
//! through `item_move::members::widen_members`. Only the origin and its ancestor modules are read
//! (F4): a member a module outside them holds cannot be named by the moved code.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::{Range, RangeInclusive};

use serde_json::Value;

use super::super::item_move::destination::{Module, Package};
use super::super::item_move::members::{
    members_of, widen_members, Member, MemberWidening, ReachedMember,
};
use super::super::item_move::outline::{root_items, visibility_edit, Item};
use super::super::item_move::scope::Scope;
use super::super::item_move::sites::module_of_file;
use super::super::item_move::text::{enclosing_modules, identifiers_in, line_end, line_start};
use super::super::retarget_impl::outline::Run;
use super::super::{seam_refusal, RustBackend};
use crate::edit::VisibilityChange;
use crate::registry::Workspace;
use crate::Result;

/// Where the survey reads from: the origin file, its run, and the destination.
pub(super) struct Survey<'a> {
    pub(super) package: &'a Package,
    pub(super) uri: &'a str,
    pub(super) file: &'a str,
    pub(super) text: &'a str,
    pub(super) symbols: &'a Value,
    pub(super) run: &'a Run,
    /// The origin module, below the crate root.
    pub(super) source: &'a [String],
    pub(super) destination: &'a Module,
}

/// What the survey found: the root items the moved code names (its imports and widenings), the
/// widenings of its members and items, and the text of each other file a widening edits, by path.
pub(super) struct Reach {
    pub(super) items: Vec<Item>,
    pub(super) widening: MemberWidening,
    pub(super) texts: BTreeMap<String, String>,
}

mod stayed;

impl RustBackend {
    /// The items the moved code reaches and the widenings the move needs (R4, R6 a–d).
    pub(super) fn survey_members(
        &mut self,
        workspace: &Workspace<'_>,
        survey: &Survey<'_>,
    ) -> Result<Reach> {
        let lines = member_lines(survey.run)?;
        let region = line_start(survey.text, *lines.start())..line_end(survey.text, *lines.end());
        let mentioned = identifiers_in(&survey.text[region.clone()]);
        let items = self.reached_items(survey, &lines, &mentioned)?;
        let mut reach = Reach {
            items: items.clone(),
            widening: MemberWidening::default(),
            texts: BTreeMap::new(),
        };

        let moved = self.moved_members(workspace, survey, &region)?;
        absorb(
            &mut reach.widening,
            widen_members(survey.text, &moved)?,
            None,
        );
        if !survey.destination.path.starts_with(survey.source) {
            self.stayed_members(workspace, survey, (&lines, &region, &mentioned), &mut reach)?;
        }
        let reached =
            reached_widenings(survey.text, survey.source, &survey.destination.path, &items)?;
        absorb(&mut reach.widening, reached, None);
        Ok(reach)
    }

    /// The root items the moved lines name (R4), confirmed by the server, and the origin's own self
    /// type, which the block's header names in the destination.
    ///
    /// A self type the origin imports rather than defines is not in this list: it reaches the
    /// destination through the origin's `use` header, which `imports::needed` copies whole.
    fn reached_items(
        &mut self,
        survey: &Survey<'_>,
        lines: &RangeInclusive<u32>,
        mentioned: &BTreeSet<String>,
    ) -> Result<Vec<Item>> {
        let mut reached = Vec::new();
        for item in root_items(survey.symbols, survey.text, None) {
            let is_self_type = item.name == survey.run.self_type;
            let names_it = !is_self_type
                && mentioned.contains(&item.name)
                && self.referenced_within(survey, lines, &item.position)?;
            if is_self_type || names_it {
                reached.push(item);
            }
        }
        Ok(reached)
    }

    /// Whether the server reports a reference to the item at `position` inside the moved lines.
    fn referenced_within(
        &mut self,
        survey: &Survey<'_>,
        lines: &RangeInclusive<u32>,
        position: &Value,
    ) -> Result<bool> {
        let references = self.references_at(survey.uri, position)?;
        Ok(references
            .as_array()
            .into_iter()
            .flatten()
            .any(|reference| {
                reference.get("uri").and_then(Value::as_str) == Some(survey.uri)
                    && reference
                        .pointer("/range/start/line")
                        .and_then(Value::as_u64)
                        .is_some_and(|line| lines.contains(&(line as u32 + 1)))
            }))
    }

    /// The private members the run moves, each with the modules outside the run that use it (R6 a).
    fn moved_members(
        &mut self,
        workspace: &Workspace<'_>,
        survey: &Survey<'_>,
        region: &Range<usize>,
    ) -> Result<Vec<ReachedMember>> {
        let lines = member_lines(survey.run)?;
        let members = members_of(survey.symbols, survey.text, 1..=u32::MAX, true);
        let mut reached = Vec::new();
        for member in members
            .into_iter()
            .filter(|member| in_lines(member, &lines))
        {
            let member = Member {
                owner: survey.run.self_type.clone(),
                ..member
            };
            let users = self.users_of_member(workspace, survey, &member, region)?;
            reached.push(ReachedMember {
                member,
                written_in: survey.source.to_vec(),
                lands_in: survey.destination.path.clone(),
                users,
            });
        }
        Ok(reached)
    }

    /// The modules that name `member` from outside the run: the module of each reference's file and
    /// the modules whose blocks enclose it.
    fn users_of_member(
        &mut self,
        workspace: &Workspace<'_>,
        survey: &Survey<'_>,
        member: &Member,
        region: &Range<usize>,
    ) -> Result<Vec<Vec<String>>> {
        let named = [(member.name.as_str(), &member.position)];
        let sites = self.sites_of(survey.uri, workspace, survey.file, survey.text, &named)?;
        let mut users = Vec::new();
        for site in sites
            .iter()
            .filter(|site| !(site.path == survey.file && region.contains(&site.offset)))
        {
            let text = workspace.read(&site.path)?;
            if let Some(base) =
                module_of_file(workspace.root, &survey.package.crate_name, &site.path)
            {
                users.push(
                    base.into_iter()
                        .chain(enclosing_modules(&text, site.offset))
                        .collect(),
                );
            }
        }
        Ok(users)
    }
}

/// R6 (d): a private root item of the origin that the moved lines name is widened for the
/// destination, as far as the destination needs it. This is the reached-item half of
/// `item_move::assemble::visibilities`, which the engine extracted; that function takes `item_move`'s
/// own move, so this one reads the same rule from the items alone.
fn reached_widenings(
    text: &str,
    source: &[String],
    destination: &[String],
    items: &[Item],
) -> Result<MemberWidening> {
    let mut widening = MemberWidening::default();
    for item in items {
        let written = Scope::parse(&item.visibility, source).ok_or_else(|| {
            seam_refusal(format!(
                "the visibility `{}` of `{}` is not one this move can read",
                item.visibility, item.name
            ))
        })?;
        let widened = written.clone().widened_to(destination);
        if widened != written {
            let to = widened.spelled_in(source);
            widening
                .edits
                .push(visibility_edit(text, &item.position, &item.name, &to)?);
            widening.report.push(VisibilityChange {
                item: item.name.clone(),
                from: spelled(&item.visibility),
                to: spelled(&to),
            });
        }
    }
    Ok(widening)
}

/// Adds `from` to `into`: its edits go to the file `file` names, or to the origin's when it names
/// none, and its report follows.
fn absorb(into: &mut MemberWidening, from: MemberWidening, file: Option<&str>) {
    match file {
        Some(path) => into
            .other_files
            .entry(path.to_string())
            .or_default()
            .extend(from.edits),
        None => into.edits.extend(from.edits),
    }
    into.report.extend(from.report);
}

/// The one-based first and last lines of the run's members, attached trivia included.
fn member_lines(run: &Run) -> Result<RangeInclusive<u32>> {
    let (Some(first), Some(last)) = (run.moved().first(), run.moved().last()) else {
        return Err(seam_refusal("the run moves no member"));
    };
    Ok(first.first_line..=last.last_line)
}

/// Whether a member's name is on one of `lines`.
fn in_lines(member: &Member, lines: &RangeInclusive<u32>) -> bool {
    member
        .position
        .get("line")
        .and_then(Value::as_u64)
        .is_some_and(|line| lines.contains(&(line as u32 + 1)))
}

fn spelled(visibility: &str) -> String {
    if visibility.is_empty() {
        "private".to_string()
    } else {
        visibility.to_string()
    }
}

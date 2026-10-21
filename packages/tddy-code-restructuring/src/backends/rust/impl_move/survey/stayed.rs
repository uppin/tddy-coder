//! The stayed members of a member move's type (R6 b, c): the private members of the origin's type
//! that stay, in the origin and in each ancestor module the origin sits under, which the moved lines
//! name.
//!
//! The origin's outline is the survey's own; each ancestor is opened and read here. Only the
//! ancestors strictly below the common ancestor of origin and destination are read (F4): a member a
//! module above that point holds is one the destination already reaches.

use std::collections::BTreeSet;
use std::ops::{Range, RangeInclusive};

use super::super::super::item_move::destination::{find_module, Lookup};
use super::super::super::item_move::members::{
    members_of, widen_members, Member, MemberWidening, ReachedMember,
};
use super::super::super::{failure, uri_of, RustBackend};
use super::{absorb, in_lines, Reach, Survey};
use crate::registry::Workspace;
use crate::Result;

/// A file a stayed member is read from: its uri, path and text, and the module it is.
struct Held<'a> {
    uri: &'a str,
    file: &'a str,
    text: &'a str,
    module: &'a [String],
}

impl RustBackend {
    /// The private members of the origin's type that stay, in the origin and in each ancestor module
    /// of the origin below the common ancestor of origin and destination (R6 b, c). The destination
    /// inside the origin's subtree makes this empty, with no server request.
    pub(super) fn stayed_members(
        &mut self,
        workspace: &Workspace<'_>,
        survey: &Survey<'_>,
        (lines, region, mentioned): (&RangeInclusive<u32>, &Range<usize>, &BTreeSet<String>),
        reach: &mut Reach,
    ) -> Result<()> {
        let origin = Held {
            uri: survey.uri,
            file: survey.file,
            text: survey.text,
            module: survey.source,
        };
        let members = members_of(survey.symbols, survey.text, 1..=u32::MAX, true)
            .into_iter()
            .filter(|member| !in_lines(member, lines))
            .collect();
        let widened =
            self.widen_stayed(workspace, survey, (region, mentioned), &origin, members)?;
        absorb(&mut reach.widening, widened, None);

        let common = survey
            .source
            .iter()
            .zip(&survey.destination.path)
            .take_while(|(one, other)| one == other)
            .count();
        // TODO(reshape-move-impl-members): an ancestor's members are the ones its file declares at the
        // top level, as `members_of` reads them; a block inside an inline `mod` of an ancestor's file is
        // not surveyed. See `docs/dev/todo/2026-10-20-restructure-move-impl-members-skips-impls-in-an-ancestors-inline-module.md`.
        for depth in common + 1..survey.source.len() {
            let module = &survey.source[..depth];
            self.ancestor_stayed(workspace, survey, module, (region, mentioned), reach)?;
        }
        Ok(())
    }

    /// The widenings of the stayed members of `module`, an ancestor of the origin, read from the file
    /// that declares it. Its text and the widened edits go to `reach`, keyed by that file.
    fn ancestor_stayed(
        &mut self,
        workspace: &Workspace<'_>,
        survey: &Survey<'_>,
        module: &[String],
        (region, mentioned): (&Range<usize>, &BTreeSet<String>),
        reach: &mut Reach,
    ) -> Result<()> {
        let Lookup::Found(ancestor) = find_module(workspace, survey.package, module)? else {
            return Err(failure(format!(
                "the module `{}` that holds the origin is not found",
                module.join("::")
            )));
        };
        let text = workspace.read(&ancestor.file)?;
        let uri = uri_of(&workspace.root.join(&ancestor.file));
        self.did_open(&uri, &text)?;
        let symbols = self.settled_outline(&uri)?;
        let held = Held {
            uri: &uri,
            file: &ancestor.file,
            text: &text,
            module,
        };
        let members = members_of(&symbols, &text, 1..=u32::MAX, true);
        let widened = self.widen_stayed(workspace, survey, (region, mentioned), &held, members)?;
        absorb(&mut reach.widening, widened, Some(&ancestor.file));
        reach.texts.insert(ancestor.file.clone(), text);
        Ok(())
    }

    /// The widenings of the `members` of the origin's type that `held` holds and the moved lines name.
    /// The destination is the one module the moved code reaches them from.
    fn widen_stayed(
        &mut self,
        workspace: &Workspace<'_>,
        survey: &Survey<'_>,
        (region, mentioned): (&Range<usize>, &BTreeSet<String>),
        held: &Held<'_>,
        members: Vec<Member>,
    ) -> Result<MemberWidening> {
        let mut reached = Vec::new();
        for member in members.into_iter().filter(|member| {
            bare_type(&member.owner) == survey.run.self_type && mentioned.contains(&member.name)
        }) {
            let member = Member {
                owner: survey.run.self_type.clone(),
                ..member
            };
            let named = [(member.name.as_str(), &member.position)];
            let sites = self.sites_of(held.uri, workspace, held.file, held.text, &named)?;
            let from_the_moved_lines = sites
                .iter()
                .any(|site| site.path == survey.file && region.contains(&site.offset));
            let users = if from_the_moved_lines {
                vec![survey.destination.path.clone()]
            } else {
                Vec::new()
            };
            reached.push(ReachedMember {
                member,
                written_in: held.module.to_vec(),
                lands_in: held.module.to_vec(),
                users,
            });
        }
        widen_members(held.text, &reached)
    }
}

/// The type a member's owner names, without the generic parameters the outline reports with it:
/// `Host` for `<T> Host<T>`.
fn bare_type(owner: &str) -> &str {
    let owner = owner.trim();
    let mut start = 0;
    if owner.starts_with('<') {
        let mut depth = 0usize;
        for (at, character) in owner.char_indices() {
            match character {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        start = at + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    owner[start..]
        .trim()
        .split('<')
        .next()
        .unwrap_or_default()
        .trim()
}

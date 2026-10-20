//! Putting a member move together: the new text of every file it touches.
//!
//! A function of the texts alone: the origin and destination files, the run of members, the root
//! items the moved members name, and the widenings of `item_move`'s member piece. Nothing here reads
//! a file or asks a server. The members' bytes are copied from the origin; only a visibility or a
//! relative spelling inside them is edited (R5, R6). Landing (R3) is in [`landing`].

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use super::super::crate_move::source_scan::attached_trivia_starts_at;
use super::super::early_return::masked_to_code;
use super::super::item_move::assemble::written_from_the_root;
use super::super::item_move::destination::Module;
use super::super::item_move::imports::{self, Source};
use super::super::item_move::members::MemberWidening;
use super::super::item_move::outline::Item;
use super::super::item_move::preflight::names_declared_in;
use super::super::item_move::rebase::{self, Modules};
use super::super::item_move::text::{
    applied, line_end, line_start, next_line_start, use_insertion, Edit,
};
use super::super::retarget_impl::outline::Run;
use super::super::{failure, seam_refusal};
use super::landing::{attributes_start, destination_edits, line_start_of, placed, Landing, Origin};
use crate::edit::VisibilityChange;
use crate::Result;

/// What a member move is made of.
pub(in crate::backends::rust) struct MovingMembers<'a> {
    pub(in crate::backends::rust) source_file: &'a str,
    pub(in crate::backends::rust) source_text: &'a str,
    /// The origin module, below the crate root.
    pub(in crate::backends::rust) source: &'a [String],
    /// The block the members leave and which of its members move.
    pub(in crate::backends::rust) run: &'a Run,
    /// Where the members land (a created module's file is not on disk yet, and its scope is empty).
    pub(in crate::backends::rust) destination: &'a Module,
    /// The destination file's text; empty for a created module.
    pub(in crate::backends::rust) destination_text: &'a str,
    /// The parent that declares the destination, and its name, when the move creates it.
    pub(in crate::backends::rust) created: Option<(&'a Module, &'a str)>,
    /// The root items of the origin the moved members name (the self type among them when the
    /// origin defines it): each gets a `use` in the destination.
    pub(in crate::backends::rust) reached: &'a [Item],
    /// The visibility edits (in origin coordinates) and report lines of R6.
    pub(in crate::backends::rust) widening: &'a MemberWidening,
}

/// The result of a member move: the new text of each file it changes.
pub(in crate::backends::rust) struct Assembled {
    pub(in crate::backends::rust) files: BTreeMap<String, (String, String)>,
    pub(in crate::backends::rust) report: Vec<VisibilityChange>,
    pub(in crate::backends::rust) notes: Vec<String>,
}

/// The `impl` block the run leaves, located in the origin text: where its attributes and its body
/// begin, the indentation of its header line, and its header as the notes name it.
struct OriginBlock {
    attributes: usize,
    body: usize,
    indent: String,
    label: String,
}

/// The new text of each file a member move changes, with the report and the notes of the move.
pub(in crate::backends::rust) fn assemble(moving: &MovingMembers<'_>) -> Result<Assembled> {
    // TODO(reshape-move-impl-members): the parent that declares a created module is not one of the
    // inputs; milestone 4 settles how that declaration reaches the assembly.
    if moving.created.is_some() {
        return Err(failure(
            "a member move that creates its module (`name`) is not assembled yet",
        ));
    }
    let origin = origin_block(moving.source_text, moving.run)?;
    let members = members_region(moving.source_text, moving.run)?;
    let (inside, outside) = split_widening(&moving.widening.edits, &members);
    let moved = moved_text(moving, &members, &inside)?;
    let (landed, landing) = landed(moving, &origin, &moved)?;
    let cut = Edit::replace(cut_span(moving.source_text, moving.run)?, "");
    let mut notes = landing_notes(moving, &landing, &origin);
    let imports = imports_for(moving, &mut notes);
    let mut by_file: BTreeMap<String, Vec<Edit>> = BTreeMap::new();
    by_file
        .entry(moving.source_file.to_string())
        .or_default()
        .extend(std::iter::once(cut).chain(outside));
    by_file
        .entry(moving.destination.file.clone())
        .or_default()
        .extend(destination_edits(imports, landed));

    Ok(Assembled {
        files: changed_files(moving, by_file)?,
        report: moving.widening.report.clone(),
        notes,
    })
}

/// The members' landing in the destination, and which landing it was.
fn landed(
    moving: &MovingMembers<'_>,
    origin: &OriginBlock,
    moved: &str,
) -> Result<(Edit, Landing)> {
    let header = Origin {
        header: &moving.source_text[origin.attributes..origin.body],
        indent: &origin.indent,
    };
    placed(
        moving.destination_text,
        &moving.destination.scope,
        &header,
        moved,
    )
}

/// The notes saying which block the members joined or opened, and whether an emptied block went.
fn landing_notes(
    moving: &MovingMembers<'_>,
    landing: &Landing,
    origin: &OriginBlock,
) -> Vec<String> {
    let target = written_from_the_root(&moving.destination.path);
    let label = &origin.label;
    let joined = match landing {
        Landing::Join { .. } => format!("the members joined the `{label}` block of `{target}`"),
        Landing::New => format!("the members opened a new `{label}` block in `{target}`"),
    };
    let mut notes = vec![joined];
    if moving.run.whole_block() {
        notes.push(format!(
            "the run empties the `{label}` block, which is removed"
        ));
    }
    notes
}

/// The block the run's header line opens, with its attributes, body and indentation.
fn origin_block(text: &str, run: &Run) -> Result<OriginBlock> {
    let masked = masked_to_code(text);
    let line = line_start(text, run.header_line);
    let keyword = line
        + masked[line..]
            .find("impl")
            .ok_or_else(|| seam_refusal("the header of the moved `impl` names no `impl`"))?;
    let body = keyword
        + masked[keyword..]
            .find('{')
            .ok_or_else(|| seam_refusal("the moved `impl` has no body to open"))?;
    let indent = text[line..keyword]
        .chars()
        .take_while(|character| character.is_whitespace())
        .collect();
    let label = text[keyword..body]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    Ok(OriginBlock {
        attributes: attributes_start(text, keyword),
        body,
        indent,
        label,
    })
}

/// The one-based first and last lines of the moved members (attached trivia included).
fn member_lines(run: &Run) -> Result<(u32, u32)> {
    let (Some(first), Some(last)) = (run.moved().first(), run.moved().last()) else {
        return Err(seam_refusal("the run moves no member"));
    };
    Ok((first.first_line, last.last_line))
}

/// The bytes of the moved members, from the first one's first line to the last one's end.
fn members_region(text: &str, run: &Run) -> Result<Range<usize>> {
    let (first, last) = member_lines(run)?;
    Ok(line_start(text, first)..line_end(text, last))
}

/// The span the origin loses (R2): the moved lines, or the whole block, and one blank line.
fn cut_span(text: &str, run: &Run) -> Result<Range<usize>> {
    let (first, last) = if run.whole_block() {
        (
            attached_trivia_starts_at(text, run.header_line),
            run.close_line,
        )
    } else {
        member_lines(run)?
    };
    let mut span = line_start(text, first)..line_end(text, last);
    if is_blank_line_at(text, span.end) {
        span.end = next_line_start(text, span.end);
    } else if span.start > 0 && is_blank_line_at(text, line_start_of(text, span.start - 1)) {
        span.start = line_start_of(text, span.start - 1);
    }
    Ok(span)
}

fn is_blank_line_at(text: &str, offset: usize) -> bool {
    text.get(offset..)
        .and_then(|rest| rest.lines().next())
        .is_some_and(|line| line.trim().is_empty())
}

/// The widenings that touch the moved lines, and the rest (which stay in the origin file).
fn split_widening(edits: &[Edit], members: &Range<usize>) -> (Vec<Edit>, Vec<Edit>) {
    edits
        .iter()
        .cloned()
        .partition(|edit| members.start <= edit.start && edit.end <= members.end)
}

/// The moved lines with their relative spellings kept (R5) and their widenings applied (R6), at
/// the origin's indentation.
fn moved_text(
    moving: &MovingMembers<'_>,
    members: &Range<usize>,
    widened: &[Edit],
) -> Result<String> {
    let claimed: Vec<Range<usize>> = widened.iter().map(|edit| edit.start..edit.end).collect();
    let modules = Modules {
        file: moving.source_file,
        from: moving.source,
        to: &moving.destination.path,
        travelling: None,
        imports: None,
    };
    let respelled = rebase::edits(
        moving.source_text,
        members.clone(),
        &modules,
        &BTreeSet::new(),
        &claimed,
    )?;
    let shifted: Vec<Edit> = respelled
        .into_iter()
        .chain(widened.iter().cloned())
        .map(|edit| Edit {
            start: edit.start - members.start,
            end: edit.end - members.start,
            text: edit.text,
        })
        .collect();
    applied(&moving.source_text[members.clone()], &shifted)
}

/// The `use` items the moved code needs in the destination, when there are any (R4).
fn imports_for(moving: &MovingMembers<'_>, notes: &mut Vec<String>) -> Option<Edit> {
    let scope = moving.destination.scope.clone();
    let taken = names_declared_in(&moving.destination_text[scope.clone()]);
    let names: Vec<String> = moving
        .reached
        .iter()
        .map(|item| item.name.clone())
        .collect();
    let qualifier = written_from_the_root(moving.source);
    let source = Source {
        text: moving.source_text,
        scope: 0..moving.source_text.len(),
        module: moving.source,
        qualifier: &qualifier,
    };
    let lines = imports::needed(&source, &taken, &names);
    if lines.is_empty() {
        return None;
    }
    notes.push(format!(
        "imports: {} `use` item(s) the moved code needs, copied from the origin module",
        lines.len()
    ));
    let (at, blank) = use_insertion(moving.destination_text, scope);
    let mut block: String = lines.iter().map(|line| format!("{line}\n")).collect();
    block.push_str(if blank { "\n" } else { "" });
    Some(Edit::insert(at, block))
}

/// The new text of every file the edits change, where they change it.
fn changed_files(
    moving: &MovingMembers<'_>,
    by_file: BTreeMap<String, Vec<Edit>>,
) -> Result<BTreeMap<String, (String, String)>> {
    let mut files = BTreeMap::new();
    for (path, mut edits) in by_file {
        let old = if path == moving.source_file {
            moving.source_text
        } else {
            moving.destination_text
        };
        edits.sort_by_key(|edit| (edit.start, edit.end));
        let new = applied(old, &edits)?;
        if new != old {
            files.insert(path, (old.to_string(), new));
        }
    }
    Ok(files)
}

#[cfg(test)]
mod assemble_tests;

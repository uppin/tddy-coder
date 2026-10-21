//! The edits a surveyed `detach_method` makes, and the R6 note they carry.
//!
//! Per file, every edit is a [`Splice`] over the original text. In the file that holds the block:
//! each detached member's receiver becomes `name: &mut T` (R1), its `self` and `Self` are rewritten
//! by `read_fields_through`'s self-mode rewrite (R2), and the block is replaced by what the members
//! and the members it keeps write (R3). Every call is re-pointed (R4). Nothing is written until
//! every splice is known, so a refusal leaves the tree untouched.

use std::collections::{BTreeMap, BTreeSet};

use super::super::item_move::text::line_start;
use super::super::read_fields_through::edits;
use super::super::read_fields_through::range::{self_sites, SelfSite};
use super::super::seam_survey::minimal_edits;
use super::block::Block;
use super::calls::{Reach, UseLine};
use super::member::Receiver;
use super::splice::{render, Splice};
use super::survey::{Call, Surveyed};
use crate::edit::{FileEdit, Resolution, WorkspaceEdit};
use crate::Result;

/// The edits of a surveyed operation, with its note, for a survey with no findings.
pub(super) fn resolution(surveyed: Surveyed) -> Result<Resolution> {
    let Surveyed {
        file,
        texts,
        parameter,
        block,
        sites,
        findings: _,
    } = surveyed;

    let mut splices: BTreeMap<String, Vec<Splice>> = BTreeMap::new();
    let mut inserted_uses: BTreeSet<(String, usize)> = BTreeSet::new();
    let mut files: BTreeSet<&str> = BTreeSet::new();
    let mut use_lines = 0usize;
    for site in &sites {
        files.insert(site.file.as_str());
        let list = splices.entry(site.file.clone()).or_default();
        let (written, uses) = match &site.reach {
            Reach::Here => (String::new(), None),
            Reach::Through { written, uses } => (written.clone(), uses.as_ref()),
        };
        match &site.call {
            Call::Method {
                span,
                receiver,
                lend,
                args,
            } => list.push(Splice::Call {
                span: span.clone(),
                written,
                method: block.members[site.member].name.clone(),
                lend: *lend,
                receiver: receiver.clone(),
                args: args.clone(),
            }),
            Call::Path { qualifier } => list.push(Splice::Text {
                span: qualifier.clone(),
                text: written,
            }),
        }
        if let Some(line) = uses {
            if inserted_uses.insert((site.file.clone(), line.at)) {
                list.push(Splice::Text {
                    span: line.at..line.at,
                    text: insertion_of(line),
                });
                use_lines += 1;
            }
        }
    }

    let block_file = texts.get(&file).map(String::as_str).unwrap_or_default();
    let anchored = splices.entry(file.clone()).or_default();
    for index in block.detached.clone() {
        let member = &block.members[index];
        let receiver = member.receiver.as_ref().map(Receiver::span);
        if let Some(Receiver::Borrowed {
            mutable,
            lifetime,
            span,
        }) = &member.receiver
        {
            let lifetime = lifetime
                .as_deref()
                .map_or(String::new(), |name| format!("{name} "));
            anchored.push(Splice::Text {
                span: span.clone(),
                text: format!(
                    "{parameter}: &{lifetime}{}{}",
                    if *mutable { "mut " } else { "" },
                    block.self_type
                ),
            });
        }
        let own_sites: Vec<SelfSite> = self_sites(block_file, member.span.clone())
            .into_iter()
            .filter(|site| !receiver.is_some_and(|span| span.contains(&site.offset)))
            .collect();
        for (span, replacement) in edits::rewrites(
            block_file,
            member.span.clone(),
            &parameter,
            "self",
            &block.self_type,
            &own_sites,
            &BTreeSet::new(),
        ) {
            if !span.is_empty() {
                anchored.push(Splice::Text {
                    span,
                    text: replacement,
                });
            }
        }
    }

    let known = splices.get(&file).cloned().unwrap_or_default();
    let (block_text, emptied) = block_text(block_file, &block, &known);
    splices.entry(file.clone()).or_default().push(Splice::Text {
        span: line_start(block_file, block.header_line)
            ..line_start(block_file, block.close_line + 1),
        text: block_text,
    });

    let mut changes = Vec::new();
    for (path, list) in &splices {
        let original = texts.get(path).map(String::as_str).unwrap_or_default();
        let rendered = render(original, 0..original.len(), list);
        if rendered != original {
            changes.push(FileEdit::Change {
                path: path.clone(),
                edits: minimal_edits(original, &rendered),
            });
        }
    }

    let note = note_of(
        &block,
        &parameter,
        sites.len(),
        files.len(),
        use_lines,
        emptied,
    );
    Ok(Resolution {
        edit: WorkspaceEdit { changes },
        report: Vec::new(),
        notes: vec![note],
    })
}

/// The line a `use` is inserted as, with the blank line that follows it when the module had none.
fn insertion_of(line: &UseLine) -> String {
    if line.blank {
        format!("{}\n\n", line.text)
    } else {
        format!("{}\n", line.text)
    }
}

/// The block replaced by what it keeps and the functions its detached members become, and whether
/// nothing is kept. The functions follow the block in member order (R3); a block with no member
/// left is replaced by them alone.
fn block_text(text: &str, block: &Block, splices: &[Splice]) -> (String, bool) {
    let detached = &block.members[block.detached.clone()];
    let header = line_start(text, block.header_line)..line_start(text, block.header_line + 1);
    let close = line_start(text, block.close_line)..line_start(text, block.close_line + 1);

    let mut kept = String::new();
    let mut cursor = header.end;
    for member in detached {
        kept.push_str(&render(text, cursor..member.span.start, splices));
        cursor = member.span.end;
    }
    kept.push_str(&render(text, cursor..close.start, splices));
    let kept = tidy(&kept);

    let indent = leading_indent(&text[header.clone()]);
    let functions = detached
        .iter()
        .map(|member| {
            let from = leading_indent(&text[member.span.clone()]);
            reindent(&render(text, member.span.clone(), splices), &from, &indent)
        })
        .collect::<Vec<String>>()
        .join("\n");

    if kept.is_empty() {
        return (functions, true);
    }
    let mut closing = text[close.clone()].to_string();
    if !closing.ends_with('\n') {
        closing.push('\n');
    }
    (
        format!("{}{kept}{closing}\n{functions}", &text[header]),
        false,
    )
}

/// The lines kept in a block, with the blank lines at either end dropped and each run of blank lines
/// between two of them reduced to one.
fn tidy(kept: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    for line in kept.lines() {
        if line.trim().is_empty() {
            if lines.last().is_some_and(|last| !last.trim().is_empty()) {
                lines.push("");
            }
        } else {
            lines.push(line);
        }
    }
    while lines.last().is_some_and(|last| last.trim().is_empty()) {
        lines.pop();
    }
    lines.iter().map(|line| format!("{line}\n")).collect()
}

/// The indentation of the first line of `text`.
fn leading_indent(text: &str) -> String {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line[..line.len() - line.trim_start().len()].to_string())
        .unwrap_or_default()
}

/// `text` with each line's `from` indentation replaced by `to`; a blank line is left blank.
fn reindent(text: &str, from: &str, to: &str) -> String {
    text.split_inclusive('\n')
        .map(|line| {
            if line.trim().is_empty() {
                "\n".to_string()
            } else {
                match line.strip_prefix(from) {
                    Some(rest) => format!("{to}{rest}"),
                    None => line.to_string(),
                }
            }
        })
        .collect()
}

/// The R6 note: the members and the parameter, the calls and files re-pointed, the `use`s added, and
/// what became of the block.
fn note_of(
    block: &Block,
    parameter: &str,
    calls: usize,
    files: usize,
    uses: usize,
    emptied: bool,
) -> String {
    let names: Vec<String> = block.members[block.detached.clone()]
        .iter()
        .map(|member| format!("{}::{}", block.self_type, member.name))
        .collect();
    let became = if names.len() == 1 {
        "became a function"
    } else {
        "became functions"
    };
    let uses = match uses {
        0 => "no `use` added".to_string(),
        1 => "1 `use` added".to_string(),
        many => format!("{many} `use`s added"),
    };
    let fate = if emptied {
        "the block was emptied and removed"
    } else {
        "the block keeps its other members"
    };
    format!(
        "detach_method: {} {became} over `{parameter}`; {} in {} re-pointed ({uses}); {fate}",
        names.join(", "),
        plural(calls, "call", "calls"),
        plural(files, "file", "files"),
    )
}

/// `count` of `one` or `many`, with the count.
fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

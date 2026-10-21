//! The `impl` block the anchored members sit in, read from the server's outline: its header, its
//! members in order, and which of them the run detaches.
//!
//! The block is read the way `retarget_impl` reads the run it moves (`outline::read`), so both
//! operations agree on what an anchor covers. What this adds is the header's shape (a generic or
//! `where`-bound block, a trait impl) and each member's declaration, receiver and visibility.

use std::ops::Range;

use serde_json::Value;

use super::super::item_move::text::line_start;
use super::super::retarget_impl::outline;
use super::super::{seam_refusal, server_defect};
use super::member::{self, Declaration, Kind, Receiver};
use super::scan;
use crate::edit::Range as LineRange;
use crate::Result;

/// The block a run detaches members of.
pub(super) struct Block {
    /// The block's self type, as its outline label names it: `Host` of `impl Host`.
    pub(super) self_type: String,
    /// The header as written, its brace left out: `impl Host`, `impl Tally for Host`.
    pub(super) label: String,
    /// Whether the header carries generic parameters or a `where` clause.
    pub(super) generic: bool,
    /// Whether the block implements a trait.
    pub(super) is_trait: bool,
    /// The one-based line the `impl` keyword is on.
    pub(super) header_line: u32,
    /// The one-based line the closing `}` is on.
    pub(super) close_line: u32,
    /// Every member of the block, in source order.
    pub(super) members: Vec<Member>,
    /// The members the run detaches, as indices into `members`.
    pub(super) detached: Range<usize>,
}

/// One member of the block.
pub(super) struct Member {
    pub(super) name: String,
    /// Where the server reports the member's name (zero-based, as the server answers).
    pub(super) position: Value,
    /// The one-based line the member's name is on.
    pub(super) name_line: u32,
    /// The bytes of the member's lines, its attached trivia included.
    pub(super) span: Range<usize>,
    pub(super) declaration: Declaration,
    /// The receiver of a function that has one.
    pub(super) receiver: Option<Receiver>,
    /// Whether the member is written plain `pub`.
    pub(super) public: bool,
}

/// The block `span` lands in and the members it covers. `anchored` is the member names an `items`
/// or `item` anchor names, checked against what the span covers; a range anchor names none.
pub(super) fn read(
    symbols: &Value,
    text: &str,
    span: LineRange,
    anchored: Option<&[String]>,
) -> Result<Block> {
    let run = outline::read(symbols, text, span)?;
    if let Some(anchored) = anchored {
        refuse_a_mismatch(&run, anchored)?;
    }

    let lines: Vec<&str> = text.lines().collect();
    let line = |number: u32| lines.get(number as usize - 1).copied().unwrap_or_default();
    if line(run.close_line).trim() != "}" {
        return Err(seam_refusal(format!(
            "the `impl` at line {} does not close with a `}}` on a line of its own: a detach \
             cannot cut around that",
            run.header_line
        )));
    }
    let Some(open_line) =
        (run.header_line..=run.close_line).find(|&number| line(number).trim_end().ends_with('{'))
    else {
        return Err(seam_refusal(format!(
            "the `impl` at line {} does not open with a `{{`: a detach cannot cut around that",
            run.header_line
        )));
    };
    let header: Vec<&str> = (run.header_line..=open_line)
        .map(|number| line(number).trim())
        .collect();
    let label = header.join(" ").trim_end_matches('{').trim().to_string();
    let after_keyword = label.trim_start_matches("impl").trim_start();
    let generic =
        after_keyword.starts_with('<') || label.split_whitespace().any(|word| word == "where");
    let is_trait = label.split_whitespace().any(|word| word == "for");

    let masked = scan::masked(text);
    let members = run
        .members
        .iter()
        .map(|member| read_member(member, text, &masked))
        .collect::<Result<Vec<_>>>()?;
    Ok(Block {
        self_type: run.self_type.clone(),
        label,
        generic,
        is_trait,
        header_line: run.header_line,
        close_line: run.close_line,
        members,
        detached: run.first_moved..run.moved_end,
    })
}

/// Refuse an anchor whose names are not the members the span covers: a detach takes exactly the
/// members it names, and covering another one by accident would detach it too.
fn refuse_a_mismatch(run: &outline::Run, anchored: &[String]) -> Result<()> {
    let mut moved: Vec<&str> = run
        .moved()
        .iter()
        .map(|member| member.name.as_str())
        .collect();
    let mut named: Vec<&str> = anchored.iter().map(String::as_str).collect();
    moved.sort_unstable();
    named.sort_unstable();
    if moved == named {
        return Ok(());
    }
    Err(seam_refusal(format!(
        "the anchor names {} but the members its lines cover are {}: a detach takes exactly the \
         members it names",
        named.join(", "),
        moved.join(", ")
    )))
}

/// One outline child, read against the file's own text.
fn read_member(member: &outline::Member, text: &str, masked: &str) -> Result<Member> {
    let span = line_start(text, member.first_line)..line_start(text, member.last_line + 1);
    let declaration =
        member::declaration_of(masked, span.clone(), &member.name).ok_or_else(|| {
            server_defect(format!(
                "the outline names `{}`, and the text it points at declares no such member",
                member.name
            ))
        })?;
    let receiver = match declaration.kind {
        Kind::Function => member::receiver_of(text, masked, &declaration),
        Kind::ConstOrType => None,
    };
    Ok(Member {
        name: member.name.clone(),
        position: member.position.clone(),
        name_line: scan::line_of(text, declaration.name_at),
        public: member::is_public(masked, span.clone()),
        span,
        declaration,
        receiver,
    })
}

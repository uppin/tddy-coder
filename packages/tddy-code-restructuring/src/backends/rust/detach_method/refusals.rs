//! R5's refusals that the text decides: what cannot leave its block, and what the module already
//! names. Each finding names its file and line and the reason, and every offending member is
//! reported, not the first.
//!
//! The call sites are the survey's; these are the members'.

use super::block::{Block, Member};
use super::member::{Kind, Receiver};
use super::scan;
use crate::crate_move::source_scan::items_of_module;

/// The findings for the block and the members the run detaches, in source order.
pub(super) fn of_block(
    block: &Block,
    file: &str,
    text: &str,
    masked: &str,
    parameter: &str,
) -> Vec<String> {
    let mut found = Vec::new();
    if block.generic {
        found.push(located(
            file,
            block.header_line,
            format!(
                "`{}` carries generic parameters or a `where` clause, which a free function would \
                 have to restate",
                block.label
            ),
        ));
    }
    for member in &block.members[block.detached.clone()] {
        found.extend(of_member(block, member, file, text, masked, parameter));
    }
    found
}

/// The findings for one detached member.
fn of_member(
    block: &Block,
    member: &Member,
    file: &str,
    text: &str,
    masked: &str,
    parameter: &str,
) -> Vec<String> {
    let mut found: Vec<String> = declaration_reasons(block, member)
        .into_iter()
        .map(|reason| located(file, member.name_line, reason))
        .collect();
    if member.declaration.kind == Kind::ConstOrType {
        return found;
    }
    if let Some(offset) = shadowing_write(member, masked, parameter) {
        let line = scan::line_of(text, offset);
        found.push(located(
            file,
            line,
            format!(
                "`{parameter}` is already written at line {line} of `{}`: the parameter would \
                 shadow it",
                member.name
            ),
        ));
    }
    if module_declares(text, &member.name) {
        found.push(located(
            file,
            member.name_line,
            format!(
                "the module already declares or imports `{}`: the function would be declared twice",
                member.name
            ),
        ));
    }
    found
}

/// The refusals the member's declaration alone decides, all reported at its name: a trait member,
/// an associated const or type, a receiver this run does not detach, a `pub` member.
fn declaration_reasons(block: &Block, member: &Member) -> Vec<String> {
    let shown = format!("`{}::{}`", block.self_type, member.name);
    let mut reasons = Vec::new();
    if block.is_trait {
        reasons.push(format!(
            "`{}` is a member of `{}`: trait members stay methods",
            member.name, block.label
        ));
    }
    if member.declaration.kind == Kind::ConstOrType {
        reasons.push(format!(
            "{shown} is an associated const or type: there is nothing to detach"
        ));
        return reasons;
    }
    match &member.receiver {
        Some(Receiver::ByValue { .. }) => reasons.push(format!(
            "{shown} takes `self` by value: a function over a reference cannot take it"
        )),
        Some(Receiver::Typed { .. }) => reasons.push(format!(
            "{shown} takes `self` through an explicit type: this run detaches `self`, `&self` \
             and `&mut self` only"
        )),
        Some(Receiver::Borrowed { .. }) | None => {}
    }
    if member.public {
        reasons.push(format!(
            "{shown} is `pub` and may have callers outside the workspace"
        ));
    }
    reasons
}

/// The first place the member's own text writes `parameter` as a name: a binding, a parameter or
/// a use. Its receiver and its declared name are not such a place.
fn shadowing_write(member: &Member, masked: &str, parameter: &str) -> Option<usize> {
    let receiver = member.receiver.as_ref().map(Receiver::span);
    scan::occurrences(masked, member.span.clone(), parameter)
        .into_iter()
        .find(|&offset| {
            scan::is_a_plain_name(masked, offset, offset + parameter.len())
                && offset != member.declaration.name_at
                && !receiver.is_some_and(|span| span.contains(&offset))
        })
}

/// Whether the module's own text declares or imports `name` at its top level.
pub(super) fn module_declares(text: &str, name: &str) -> bool {
    let items = items_of_module(text);
    items.defined.iter().any(|defined| defined == name)
        || items
            .uses
            .iter()
            .any(|leaf| leaf.bound_name() == Some(name))
}

/// `file:line: reason`, the form every finding takes.
pub(super) fn located(file: &str, line: u32, reason: String) -> String {
    format!("{file}:{line}: {reason}")
}

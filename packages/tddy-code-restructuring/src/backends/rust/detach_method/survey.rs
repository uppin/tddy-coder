//! The survey a `detach_method` makes before any edit is built: the block the anchor names, every
//! R5 refusal the text decides, and every call the server reports for a detached member, each read
//! as the call it is or refused with its reason.
//!
//! `check` returns the findings and `resolve` refuses with them, so both read this one survey.

use std::collections::BTreeMap;
use std::ops::Range;

use super::super::character_column;
use super::super::item_move::text::enclosing_modules;
use super::super::{failure, server_defect, LspPoint};
use super::block::{self, Block};
use super::calls::{self, Lend, Reach, Shape};
use super::member::{Kind, Receiver};
use super::refusals;
use super::scan;
use super::DetachServer;
use crate::item_anchor::module_path_of;
use crate::plan::{Anchor, ItemPath, ItemSegment, RefactorOp};
use crate::registry::Workspace;
use crate::Result;

/// What the survey found: the block, every call of a detached member, and every refusal.
pub(super) struct Surveyed {
    /// The file that holds the block.
    pub(super) file: String,
    /// The text of every file the survey read, the block's file among them.
    pub(super) texts: BTreeMap<String, String>,
    /// The parameter the receiver becomes.
    pub(super) parameter: String,
    pub(super) block: Block,
    /// The calls to rewrite, in the order the server reported them.
    pub(super) sites: Vec<Site>,
    /// Every refusal, in the order it was found; empty when the operation can run.
    pub(super) findings: Vec<String>,
}

/// One call of a detached member, to be re-pointed.
pub(super) struct Site {
    /// The file the call is written in.
    pub(super) file: String,
    /// Which member of the block it calls, an index into `block.members`.
    pub(super) member: usize,
    pub(super) call: Call,
    pub(super) reach: Reach,
}

/// How a call is written, as the rewrite needs it.
pub(super) enum Call {
    /// `receiver.member(args)`.
    Method {
        span: Range<usize>,
        receiver: Range<usize>,
        lend: Lend,
        args: Option<Range<usize>>,
    },
    /// `Self::member` / `Type::member` before the arguments: the span of the path up to the name.
    Path { qualifier: Range<usize> },
}

/// What a plain `check` finds wrong from the anchored file's text alone: a member whose name the
/// module already declares or imports. Nothing here needs the server.
pub(super) fn plain_findings(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>> {
    let (file, items): (&str, Vec<&ItemPath>) = match &op.anchor {
        Anchor::Items { file, items, .. } => (file, items.iter().collect()),
        Anchor::Item {
            file,
            item,
            start: None,
            end: None,
            ..
        } => (file, vec![item]),
        _ => return Ok(Vec::new()),
    };
    let text = workspace.read(file)?;
    Ok(items
        .into_iter()
        .map(|item| (item, member_name(item)))
        .filter(|(_, name)| refusals::module_declares(&text, name))
        .map(|(item, name)| {
            format!(
                "{file}: the module already declares or imports `{name}`: `{item}` cannot be \
                 detached, since its function would be declared twice"
            )
        })
        .collect())
}

/// The survey of one `detach_method`: the block, its refusals, and the calls to its members.
pub(super) fn survey(
    server: &mut dyn DetachServer,
    op: &RefactorOp,
    workspace: &Workspace<'_>,
) -> Result<Surveyed> {
    let file = op.anchor.file().to_string();
    let parameter = op.name.clone().ok_or_else(|| {
        failure("`detach_method` needs `name`: the parameter the receiver becomes")
    })?;
    let text = workspace.read(&file)?;
    let span = server.span(op, workspace)?;
    let symbols = server.outline(workspace, &file)?;
    let block = block::read(&symbols, &text, span, anchored_names(op).as_deref())?;
    let block_module = module_path_of(workspace.root, &file)?;

    let mut findings = refusals::of_block(&block, &file, &text, &scan::masked(&text), &parameter);
    let mut texts = BTreeMap::from([(file.clone(), text)]);
    let mut maskeds = BTreeMap::from([(file.clone(), scan::masked(&texts[&file]))]);
    let mut sites = Vec::new();

    for index in block.detached.clone() {
        let member = &block.members[index];
        if member.declaration.kind == Kind::ConstOrType {
            continue;
        }
        let at = character_column(&texts[&file], LspPoint::read(Some(&member.position))?);
        for reference in server.references(workspace, &file, at)? {
            let path = reference.path.clone();
            if !texts.contains_key(&path) {
                let read = workspace.read(&path)?;
                maskeds.insert(path.clone(), scan::masked(&read));
                texts.insert(path.clone(), read);
            }
            let referring = &texts[&path];
            let masked = &maskeds[&path];
            let offset = scan::offset_of(referring, reference.at);
            if !referring[offset..].starts_with(member.name.as_str()) {
                return Err(server_defect(format!(
                    "the server reports a reference to `{}` at line {} of {path}, which does not \
                     name it",
                    member.name, reference.at.line
                )));
            }
            let line = scan::line_of(referring, offset);
            let shape =
                match calls::shape_of(referring, masked, offset, &member.name, &block.self_type) {
                    Ok(shape) => shape,
                    Err(reason) => {
                        findings.push(refusals::located(&path, line, reason));
                        continue;
                    }
                };
            let call = match &shape {
                Shape::Method {
                    receiver,
                    receiver_name,
                    span,
                    args,
                    ..
                } => {
                    let hover = server.hover(
                        workspace,
                        &path,
                        scan::position_of(referring, receiver.start),
                    )?;
                    let receiver_type = calls::type_of(&hover, receiver_name)?;
                    let mutable = matches!(
                        member.receiver,
                        Some(Receiver::Borrowed { mutable: true, .. })
                    );
                    match calls::lend_of(&shape, &receiver_type, &block.self_type, mutable) {
                        Ok(lend) => Call::Method {
                            span: span.clone(),
                            receiver: receiver.clone(),
                            lend,
                            args: args.clone(),
                        },
                        Err(reason) => {
                            findings.push(refusals::located(&path, line, reason));
                            continue;
                        }
                    }
                }
                Shape::Path { qualifier, .. } => Call::Path {
                    qualifier: qualifier.clone(),
                },
            };
            let prefix = match &shape {
                Shape::Path { prefix, .. } => prefix.as_deref(),
                Shape::Method { .. } => None,
            };
            let mut caller_module = module_path_of(workspace.root, &path)?;
            caller_module.extend(enclosing_modules(referring, offset));
            sites.push(Site {
                file: path.clone(),
                member: index,
                call,
                reach: calls::reach(&block_module, &caller_module, referring, offset, prefix),
            });
        }
    }

    Ok(Surveyed {
        file,
        texts,
        parameter,
        block,
        sites,
        findings,
    })
}

/// The member names an `items` or `item` anchor names; a range names none, and its members are the
/// ones its lines cover.
fn anchored_names(op: &RefactorOp) -> Option<Vec<String>> {
    match &op.anchor {
        Anchor::Items { items, .. } => Some(items.iter().map(member_name).collect()),
        Anchor::Item {
            item,
            start: None,
            end: None,
            ..
        } => Some(vec![member_name(item)]),
        _ => None,
    }
}

/// The member an item path names: its last segment.
fn member_name(item: &ItemPath) -> String {
    match item.segments().last() {
        Some(ItemSegment::Named(name)) => name.clone(),
        _ => item
            .to_string()
            .rsplit("::")
            .next()
            .unwrap_or_default()
            .to_string(),
    }
}

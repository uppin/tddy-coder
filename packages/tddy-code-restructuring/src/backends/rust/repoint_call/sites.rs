//! The bulk form's references: every call of one method the server knows, classified.
//!
//! The server answers *which* places name the method; the text around each one says whether it is a
//! method call (re-pointed), a comment (skipped and counted) or anything else (a path call, a
//! function pointer, an import — refused, all of them at once, before anything is written). What is
//! written is an insertion of the template's hops after each receiver, so nested and chained calls
//! compose without overlapping edits.

use std::collections::{BTreeMap, BTreeSet};

use super::super::early_return::masked_to_code;
use super::super::signature_rewrites::{edits_of, Replacement, Span};
use super::super::{
    failure, lsp_edits, seam_refusal, server_defect, uri_of, LspPoint, RustBackend,
};
use super::receivers;
use crate::crate_move::{readable_spans, Prose};
use crate::edit::{FileEdit, Resolution, WorkspaceEdit};
use crate::item_anchor::unlowered_item_anchor;
use crate::plan::rust_syntax;
use crate::plan::{Anchor, RefactorOp};
use crate::registry::Workspace;
use crate::{RestructureError, Result};

/// Insert the template's hops after the receiver of every call of the anchored method, in every
/// file the server knows, refusing every reference that is not a method call all at once and
/// writing nothing.
pub(super) fn repoint_receivers(
    backend: &mut RustBackend,
    op: &RefactorOp,
    workspace: &Workspace<'_>,
) -> Result<Resolution> {
    let Anchor::Range { file, start, .. } = &op.anchor else {
        return Err(unlowered_item_anchor(&op.anchor, "RepointCall"));
    };
    let callee = op
        .callee
        .as_deref()
        .ok_or_else(|| failure("`repoint_call` needs `callee`"))?;
    let text = workspace.read(file)?;

    // The lowered anchor is the method's own name; the method has to be a method, and the template
    // has to end in that name, before any server is asked.
    let name_offset = lsp_edits::offset_of(
        &text,
        LspPoint::read(Some(&lsp_edits::lsp_position(*start)))?,
    );
    let name = identifier_at(&text, name_offset)?;
    refuse_a_non_method(&text, name_offset)?;
    let hops: String = rust_syntax::one_receiver_template(callee, &name)?.concat();

    let uri = uri_of(&workspace.root.join(file));
    backend.start(workspace.root)?;
    backend.did_open(&uri, &text)?;
    backend.ensure_indexed(&uri)?;

    let position = lsp_edits::lsp_position(*start);
    let sites = backend.sites_of(&uri, workspace, file, &text, &[(name.as_str(), &position)])?;

    let mut texts: BTreeMap<String, String> = BTreeMap::new();
    texts.insert(file.clone(), text);
    let mut by_file: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut refusals: Vec<(String, usize)> = Vec::new();
    let mut comments: BTreeSet<(String, usize)> = BTreeSet::new();
    let mut re_pointed = 0usize;

    for site in &sites {
        if !texts.contains_key(&site.path) {
            texts.insert(site.path.clone(), workspace.read(&site.path)?);
        }
        let source = &texts[&site.path];
        if in_a_comment(source, site.offset) {
            comments.insert((site.path.clone(), site.offset));
            continue;
        }
        let masked = masked_to_code(source);
        if !is_a_method_call(&masked, site.offset, &site.name) {
            refusals.push((site.path.clone(), line_of(source, site.offset)));
            continue;
        }
        let at = receivers::insertions_for(source, site.offset, &hops)?;
        by_file.entry(site.path.clone()).or_default().push(at);
        re_pointed += 1;
    }

    // rust-analyzer reports no position for a doc-link target, so a comment naming the method is read
    // from the text of the files this run looked at rather than from the reference set, and skipped
    // like a comment site the server did report.
    let skipped_comments = comments.len()
        + texts
            .iter()
            .map(|(path, source)| comments_naming(source, path, &name, &comments))
            .sum::<usize>();

    if !refusals.is_empty() {
        refusals.sort();
        refusals.dedup();
        let named: Vec<String> = refusals
            .iter()
            .map(|(path, line)| format!("{path}:{line}"))
            .collect();
        return Err(seam_refusal(format!(
            "the anchor names every call of `{name}`, and {} of its references are not method calls \
             — a path call, a function pointer or an import cannot be re-pointed — so nothing is \
             written: {}",
            named.len(),
            named.join(", ")
        )));
    }

    let mut changes = Vec::new();
    for (path, offsets) in by_file {
        let source = &texts[&path];
        let edits = edits_of(
            source,
            offsets
                .into_iter()
                .map(|at| Replacement {
                    span: Span { from: at, to: at },
                    with: hops.clone(),
                })
                .collect(),
        );
        changes.push(FileEdit::Change { path, edits });
    }

    Ok(Resolution {
        edit: WorkspaceEdit { changes },
        report: Vec::new(),
        notes: vec![the_note(&name, re_pointed, skipped_comments)],
    })
}

/// The note an apply prints: how many calls were re-pointed, and how many references sat in a
/// comment and were left alone.
fn the_note(method: &str, re_pointed: usize, comments: usize) -> String {
    let skipped = match comments {
        0 => String::new(),
        1 => "; 1 reference in a comment was skipped".to_string(),
        many => format!("; {many} references in comments were skipped"),
    };
    if re_pointed == 0 {
        return format!("`{method}` has no call to re-point{skipped}");
    }
    format!("`{method}`: {re_pointed} call(s) re-pointed{skipped}")
}

/// Whether the name token at `offset` is the member of a method call: `.name` (optionally with a
/// turbofish) immediately followed by its argument list.
fn is_a_method_call(masked: &str, offset: usize, name: &str) -> bool {
    let Some(before) = masked.get(..offset) else {
        return false;
    };
    if !before.ends_with('.') || before.ends_with("..") {
        return false;
    }
    masked
        .get(offset + name.len()..)
        .is_some_and(|after| after.starts_with('(') || after.starts_with("::<"))
}

/// Whether `offset` sits inside a comment in `text`.
fn in_a_comment(text: &str, offset: usize) -> bool {
    readable_spans(text)
        .into_iter()
        .any(|(span, kind)| kind == Prose::Comment && span.contains(&offset))
}

/// How many whole-word occurrences of `name` sit inside a comment in `text` and are not already
/// counted at a server-reported site.
fn comments_naming(
    text: &str,
    path: &str,
    name: &str,
    already: &BTreeSet<(String, usize)>,
) -> usize {
    let mut count = 0;
    for (span, kind) in readable_spans(text) {
        if kind != Prose::Comment {
            continue;
        }
        let body = &text[span.clone()];
        let mut at = 0;
        while let Some(found) = body[at..].find(name) {
            let offset = span.start + at + found;
            at += found + name.len();
            if is_a_whole_word(text, offset, name.len())
                && !already.contains(&(path.to_string(), offset))
            {
                count += 1;
            }
        }
    }
    count
}

/// Whether `text[offset..offset + len]` is a whole word: no identifier character on either side.
fn is_a_whole_word(text: &str, offset: usize, len: usize) -> bool {
    let bytes = text.as_bytes();
    let bounded = |at: Option<&u8>| at.is_none_or(|byte| !is_identifier(*byte));
    bounded(offset.checked_sub(1).and_then(|at| bytes.get(at))) && bounded(bytes.get(offset + len))
}

/// The one-based line `offset` is on.
fn line_of(text: &str, offset: usize) -> usize {
    text[..offset].matches('\n').count() + 1
}

/// The identifier `offset` starts, refusing a position that starts none.
fn identifier_at(text: &str, offset: usize) -> Result<String> {
    let name: String = text
        .get(offset..)
        .unwrap_or_default()
        .chars()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .collect();
    if name.is_empty() {
        return Err(server_defect(format!(
            "the anchor names no method at offset {offset}: no identifier begins there"
        )));
    }
    Ok(name)
}

/// Refuse an anchor whose method has no `self` receiver: there is nothing to insert the hops after.
fn refuse_a_non_method(text: &str, name_offset: usize) -> Result<()> {
    let signature = the_signature(text, name_offset)?;
    let item: syn::ItemFn =
        syn::parse_str(&format!("fn f{signature} {{}}")).map_err(|_| a_non_method())?;
    if matches!(item.sig.inputs.first(), Some(syn::FnArg::Receiver(_))) {
        return Ok(());
    }
    Err(a_non_method())
}

/// The `(&self, …)` parameter list of the function whose name starts at `name_offset`, generics and
/// whitespace skipped.
fn the_signature(text: &str, name_offset: usize) -> Result<String> {
    let masked = masked_to_code(text);
    let code = masked.as_bytes();
    let mut at = name_offset;
    while code.get(at).copied().is_some_and(is_identifier) {
        at += 1;
    }
    at = skip_whitespace_forward(code, at);
    if code.get(at) == Some(&b'<') {
        at = after_angles(code, at)?;
        at = skip_whitespace_forward(code, at);
    }
    if code.get(at) != Some(&b'(') {
        return Err(a_non_method());
    }
    let close = closing_paren(code, at)?;
    Ok(masked[at..=close].to_string())
}

/// The offset just past the `<…>` list opening at `open`.
fn after_angles(code: &[u8], open: usize) -> Result<usize> {
    let mut depth = 0usize;
    for (at, byte) in code.iter().enumerate().skip(open) {
        match byte {
            b'<' => depth += 1,
            b'>' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Ok(at + 1);
                }
            }
            _ => {}
        }
    }
    Err(a_non_method())
}

/// The offset of the `)` closing the `(` at `open`.
fn closing_paren(code: &[u8], open: usize) -> Result<usize> {
    let mut depth = 0usize;
    for (at, byte) in code.iter().enumerate().skip(open) {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(at);
                }
            }
            _ => {}
        }
    }
    Err(a_non_method())
}

fn a_non_method() -> RestructureError {
    seam_refusal(
        "the anchor names no method: the bulk form re-points the receiver of every call of a method, \
         and an associated function has no receiver to re-point",
    )
}

/// The first offset at or after `at` that is not whitespace.
fn skip_whitespace_forward(code: &[u8], at: usize) -> usize {
    let mut start = at;
    while start < code.len() && code[start].is_ascii_whitespace() {
        start += 1;
    }
    start
}

fn is_identifier(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

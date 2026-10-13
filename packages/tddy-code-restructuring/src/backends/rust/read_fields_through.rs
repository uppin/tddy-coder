//! `read_fields_through`: rebinding the `self` a range of a method reads to a local inserted before
//! it, so the range names no `self` and can be extracted as a free function.
//!
//! **Field mode** (`expr` is a borrowed view of the host's fields, `self.agent_roster_state()`):
//! every `self.<field>` in the range becomes `<name>.<field>`, each field is checked against the
//! view's type on a trial text, and a `&` the view already provides is dropped. **Self mode**
//! (`expr` is `self`, `&*self` or `&mut *self`): every `self` becomes `<name>`, method receivers
//! included, and every `Self` becomes the enclosing impl's self type. See
//! `docs/dev/1-WIP/2026-10-09-reshape-methods-leave-type.md` for the rules (RS1-RS7).
//!
//! `range` and `edits` are `pub(crate)`: `#reshape` 18's `detach_method` reuses the self-mode
//! rewrite and the shadowing refusal from a sibling module.

pub(crate) mod edits;
pub(crate) mod range;
mod typing;

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use serde_json::{json, Value};

use super::early_return::{byte_offset, masked_to_code};
use super::lsp_edits::{offset_of, position_at};
use super::{failure, seam_refusal, uri_of, LspPoint, RustBackend, SYMBOL_KIND_IMPL};
use crate::edit::{FileEdit, Position, Range as TextRange, Resolution, TextEdit, WorkspaceEdit};
use crate::item_anchor::unlowered_item_anchor;
use crate::plan::{Anchor, RefactorOp};
use crate::registry::Workspace;
use crate::Result;
use range::{Mode, SelfSite, SelfUse};
use typing::Agreement;

/// The outline kinds of a function an `impl` holds: a method and an associated function.
const SYMBOL_KINDS_OF_FUNCTIONS: [u64; 2] = [6, 12];

/// What a `read_fields_through` line says, with the text it reads.
struct Rebind<'a> {
    file: &'a str,
    text: String,
    range: Range<usize>,
    name: &'a str,
    expr: &'a str,
    mode: Mode,
}

impl<'a> Rebind<'a> {
    /// The plan line read against the file: the range as byte offsets, and the mode `expr` selects.
    fn of(op: &'a RefactorOp, workspace: &Workspace<'_>) -> Result<Rebind<'a>> {
        let Anchor::Range { file, start, end } = &op.anchor else {
            return Err(unlowered_item_anchor(&op.anchor, "ReadFieldsThrough"));
        };
        let (Some(name), Some(expr)) = (op.name.as_deref(), op.expr.as_deref()) else {
            return Err(failure("`read_fields_through` needs `name` and `expr`"));
        };
        let text = workspace.read(file)?;
        let (Some(from), Some(to)) = (byte_offset(&text, *start), byte_offset(&text, *end)) else {
            return Err(failure(format!(
                "the range of `read_fields_through` lies outside `{file}`"
            )));
        };
        Ok(Rebind {
            file,
            range: from..to.max(from),
            text,
            name,
            expr,
            mode: range::mode_of(expr),
        })
    }

    /// RS1-RS4: the refusals the text alone gives.
    fn lexical_refusals(&self) -> Vec<String> {
        range::lexical_refusals(&self.text, self.range.clone(), self.name, self.mode)
    }
}

/// What a static check finds wrong with a `read_fields_through`, from the plan and the text alone:
/// RS1-RS4 for a `range` anchor.
pub(super) fn findings(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>> {
    if !matches!(op.anchor, Anchor::Range { .. }) || op.name.is_none() || op.expr.is_none() {
        return Ok(Vec::new());
    }
    Ok(Rebind::of(op, workspace)?.lexical_refusals())
}

impl RustBackend {
    /// Resolve a `read_fields_through` into the edit to the one file that holds the range.
    pub(super) fn read_fields_through(
        &mut self,
        op: &RefactorOp,
        workspace: &Workspace<'_>,
    ) -> Result<Resolution> {
        let rebind = Rebind::of(op, workspace)?;
        // The static refusals are re-run here, before any server, so `apply` refuses what a plain
        // `check` reports.
        let refusals = rebind.lexical_refusals();
        if !refusals.is_empty() {
            return Err(seam_refusal(refusals.join("\n")));
        }

        let uri = uri_of(&workspace.root.join(rebind.file));
        self.start(workspace.root)?;
        self.did_open(&uri, &rebind.text)?;
        self.ensure_indexed(&uri)?;
        let symbols = self.settled_outline(&uri)?;
        let method = the_method_around(&symbols, &rebind)?;

        let sites = range::self_sites(&rebind.text, rebind.range.clone());
        let (edits, note) = match rebind.mode {
            Mode::Receiver => rebind_every_self(&rebind, &method, &sites)?,
            Mode::Fields => self.rebind_fields(&uri, &rebind, &sites)?,
        };

        Ok(Resolution {
            edit: WorkspaceEdit {
                changes: vec![FileEdit::Change {
                    path: rebind.file.to_string(),
                    edits,
                }],
            },
            report: Vec::new(),
            notes: vec![note],
        })
    }

    /// Field mode: every field read through the state value, each checked against the state's type
    /// on a trial text (RS6, RS7), and the borrows the state already provides dropped.
    fn rebind_fields(
        &mut self,
        uri: &str,
        rebind: &Rebind<'_>,
        sites: &[SelfSite],
    ) -> Result<(Vec<TextEdit>, String)> {
        let Rebind {
            text, name, expr, ..
        } = rebind;
        let reads: Vec<(&SelfSite, &str)> = sites
            .iter()
            .filter_map(|site| match &site.usage {
                SelfUse::Field { name, .. } => Some((site, name.as_str())),
                _ => None,
            })
            .collect();
        let mut fields: Vec<&str> = Vec::new();
        for (_, field) in &reads {
            if !fields.contains(field) {
                fields.push(field);
            }
        }

        let host_types = self.hovered_types(uri, text, &first_reads(text, &reads, &fields))?;

        let trial = edits::rebound(
            text,
            rebind.range.clone(),
            name,
            expr,
            "",
            sites,
            &BTreeSet::new(),
        );
        let shift = name.len() as isize - "self".len() as isize;
        let inserted = trial.len() as isize - text.len() as isize - shift * reads.len() as isize;
        let in_the_trial =
            |at: usize, nth: usize| (at as isize + inserted + shift * (nth as isize + 1)) as usize;
        let state_reads: Vec<(String, usize)> = fields
            .iter()
            .map(|field| {
                let nth = reads
                    .iter()
                    .position(|(_, read)| read == field)
                    .unwrap_or(0);
                let at = field_name_offset(text, reads[nth].0);
                (field.to_string(), in_the_trial(at, nth))
            })
            .collect();

        self.did_change(uri, &trial)?;
        let state_types = self.state_types(uri, rebind, &trial, &state_reads);
        self.did_change(uri, text)?;
        let state_types = state_types?;

        let mut drop_borrow = BTreeSet::new();
        let mut differing = Vec::new();
        for (field, host) in fields.iter().zip(&host_types) {
            let state = &state_types[*field];
            match typing::agreement(host, state) {
                Agreement::Same => {}
                Agreement::ByReference => {
                    drop_borrow.insert(field.to_string());
                }
                Agreement::Differs => differing.push(format!(
                    "`self.{field}` is `{}` and `{name}.{field}` is `{}`: they differ by more than \
                     one reference",
                    typing::without_lifetimes(host),
                    typing::without_lifetimes(state)
                )),
            }
        }
        if !differing.is_empty() {
            return Err(seam_refusal(differing.join("; ")));
        }

        let rewrites = edits::rewrites(
            text,
            rebind.range.clone(),
            name,
            expr,
            "",
            sites,
            &drop_borrow,
        );
        let dropped = sites
            .iter()
            .filter(|site| match &site.usage {
                SelfUse::Field { name, borrowed } => {
                    borrowed.is_some() && drop_borrow.contains(name)
                }
                _ => false,
            })
            .count();
        Ok((
            text_edits(text, &rewrites),
            fields_note(name, reads.len(), &fields, dropped),
        ))
    }

    /// The types the server states for the field read at each offset of `text`.
    fn hovered_types(
        &mut self,
        uri: &str,
        text: &str,
        reads: &[(String, usize)],
    ) -> Result<Vec<String>> {
        reads
            .iter()
            .map(|(_, at)| {
                let position = position_at(text, *at);
                self.wait_until_resolved(uri, &position)?;
                let hover = self.request_settled(
                    "textDocument/hover",
                    json!({ "textDocument": { "uri": uri }, "position": position }),
                )?;
                typing::field_type(&hover)
            })
            .collect()
    }

    /// The state value's type of each field of `reads` (its offset in the `trial` text), or the
    /// refusal naming every field the state value lacks (RS6).
    fn state_types(
        &mut self,
        uri: &str,
        rebind: &Rebind<'_>,
        trial: &str,
        reads: &[(String, usize)],
    ) -> Result<BTreeMap<String, String>> {
        let unresolved = self.unresolved_names(uri, trial)?;
        let lacking: Vec<&str> = reads
            .iter()
            .filter(|(_, at)| {
                let position = position_at(trial, *at);
                unresolved.iter().any(|name| name.position == position)
            })
            .map(|(field, _)| field.as_str())
            .collect();
        if !lacking.is_empty() {
            let read: Vec<String> = lacking
                .iter()
                .map(|field| format!("`self.{field}`"))
                .collect();
            return Err(seam_refusal(format!(
                "`{}` has no field {}: the range reads {}",
                rebind.expr,
                lacking.join(", "),
                sentence_list(&read)
            )));
        }
        let types = self.hovered_types(uri, trial, reads)?;
        Ok(reads
            .iter()
            .map(|(field, _)| field.clone())
            .zip(types)
            .collect())
    }
}

/// Self mode: every `self` written as the binding, and every `Self` as the impl's self type.
fn rebind_every_self(
    rebind: &Rebind<'_>,
    method: &Method,
    sites: &[SelfSite],
) -> Result<(Vec<TextEdit>, String)> {
    let names_the_type = sites.iter().any(|site| site.usage == SelfUse::SelfType);
    let self_type =
        match (&method.self_type, names_the_type) {
            (Some(self_type), _) => self_type.as_str(),
            (None, false) => "",
            (None, true) => return Err(seam_refusal(
                "the range names `Self`, and the method it is in belongs to no `impl` whose self \
                 type could be written in its place",
            )),
        };
    let rewrites = edits::rewrites(
        &rebind.text,
        rebind.range.clone(),
        rebind.name,
        rebind.expr,
        self_type,
        sites,
        &BTreeSet::new(),
    );
    let note = format!(
        "read_fields_through: rebound every self ({} sites) through `{}`",
        sites.len(),
        rebind.name
    );
    Ok((text_edits(&rebind.text, &rewrites), note))
}

/// The rewrites as edits, each at its own place: a span stays a span of its own so that the ledger
/// can translate the positions between them.
fn text_edits(text: &str, rewrites: &[(Range<usize>, String)]) -> Vec<TextEdit> {
    let position = |offset: usize| {
        let line_start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
        Position {
            line: text[..offset].matches('\n').count() as u32 + 1,
            col: text[line_start..offset].chars().count() as u32 + 1,
        }
    };
    rewrites
        .iter()
        .map(|(span, new_text)| TextEdit {
            range: TextRange {
                start: position(span.start),
                end: position(span.end),
            },
            new_text: new_text.clone(),
        })
        .collect()
}

/// The note of a field-mode rebind.
fn fields_note(name: &str, reads: usize, fields: &[&str], dropped: usize) -> String {
    let plural =
        |count: usize, noun: &str| format!("{count} {noun}{}", if count == 1 { "" } else { "s" });
    let mut note = format!(
        "read_fields_through: rebound {} of self through `{name}` (fields {})",
        plural(reads, "read"),
        fields.join(", ")
    );
    if dropped > 0 {
        note.push_str(&format!("; dropped {}", plural(dropped, "borrow")));
    }
    note
}

/// `items` as a sentence: `a`, `a and b`, `a, b and c`.
fn sentence_list(items: &[String]) -> String {
    match items.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// The offset of the field a `self.<field>` read names.
fn field_name_offset(text: &str, site: &SelfSite) -> usize {
    let after_self = site.offset + "self".len();
    let dot = after_self + text[after_self..].find('.').unwrap_or(0);
    let rest = &text[dot + 1..];
    dot + 1 + (rest.len() - rest.trim_start().len())
}

/// The first read of each of `fields`, as `(field, offset of its name)`.
fn first_reads(text: &str, reads: &[(&SelfSite, &str)], fields: &[&str]) -> Vec<(String, usize)> {
    fields
        .iter()
        .filter_map(|field| {
            reads
                .iter()
                .find(|(_, read)| read == field)
                .map(|(site, _)| (field.to_string(), field_name_offset(text, site)))
        })
        .collect()
}

/// What the outline says of the method a range sits in.
struct Method {
    /// The enclosing `impl`'s self type as its header writes it, generics included.
    self_type: Option<String>,
}

/// RS5: the deepest function of the outline around the range must take a `self` receiver.
fn the_method_around(symbols: &Value, rebind: &Rebind<'_>) -> Result<Method> {
    let line = |offset: usize| rebind.text[..offset].matches('\n').count();
    let (start, end) = (line(rebind.range.start), line(rebind.range.end));
    let mut found = None;
    around(symbols, start, end, None, &mut found);

    let refusal = || seam_refusal("the range is in no method with a `self` receiver");
    let (function, label) = found.ok_or_else(refusal)?;
    let name_at = function
        .pointer("/selectionRange/start")
        .map(|start| LspPoint::read(Some(start)))
        .transpose()?
        .map(|point| offset_of(&rebind.text, point))
        .ok_or_else(refusal)?;
    if !takes_self(&masked_to_code(&rebind.text), name_at) {
        return Err(refusal());
    }
    Ok(Method {
        self_type: label.and_then(|label| self_type_of(&label)),
    })
}

/// Descend through the outline's nodes that hold lines `start..=end` (zero-based), noting the last
/// function met and the label of the `impl` it sits in.
fn around<'a>(
    nodes: &'a Value,
    start: usize,
    end: usize,
    label: Option<String>,
    found: &mut Option<(&'a Value, Option<String>)>,
) {
    for node in nodes.as_array().into_iter().flatten() {
        let line = |which: &str| {
            node.pointer(&format!("/range/{which}/line"))
                .and_then(Value::as_u64)
                .map(|line| line as usize)
        };
        let (Some(first), Some(last)) = (line("start"), line("end")) else {
            continue;
        };
        if first > start || last < end {
            continue;
        }
        let kind = node.get("kind").and_then(Value::as_u64);
        let mut label = label.clone();
        if kind == Some(SYMBOL_KIND_IMPL) {
            label = node.get("name").and_then(Value::as_str).map(str::to_string);
        } else if kind.is_some_and(|kind| SYMBOL_KINDS_OF_FUNCTIONS.contains(&kind)) {
            *found = Some((node, label.clone()));
        }
        if let Some(children) = node.get("children") {
            around(children, start, end, label, found);
        }
    }
}

/// The self type an `impl`'s outline label writes: `Host` of `impl Host`, `Wrapper<T>` of
/// `impl<T> Wrapper<T>`, `Host` of `impl Display for Host`.
fn self_type_of(label: &str) -> Option<String> {
    let rest = label.strip_prefix("impl")?.trim_start();
    let rest = if rest.starts_with('<') {
        let mut depth = 0usize;
        let mut previous = ' ';
        let end = rest.char_indices().find_map(|(at, character)| {
            match character {
                '<' => depth += 1,
                '>' if previous != '-' => depth = depth.saturating_sub(1),
                _ => {}
            }
            previous = character;
            (depth == 0).then_some(at + 1)
        })?;
        rest[end..].trim_start()
    } else {
        rest
    };
    let written = rest.rsplit(" for ").next()?;
    let written = written.split(" where").next()?.trim();
    (!written.is_empty()).then(|| written.to_string())
}

/// Whether the function whose name is at `name_at` of `masked` takes `self` as its first parameter.
fn takes_self(masked: &str, name_at: usize) -> bool {
    let rest = &masked[name_at..];
    let after_name = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    let mut at = after_name;
    let bytes = rest.as_bytes();
    let mut depth = 0usize;
    // Past the generics, to the parenthesis that opens the parameters.
    while at < bytes.len() {
        match bytes[at] {
            b'<' => depth += 1,
            b'>' if at > 0 && bytes[at - 1] != b'-' => depth = depth.saturating_sub(1),
            b'(' if depth == 0 => break,
            _ => {}
        }
        at += 1;
    }
    let parameters = &rest[(at + 1).min(rest.len())..];
    let first = parameters.split([',', ')']).next().unwrap_or_default();
    let pattern = first.split(':').next().unwrap_or_default();
    pattern
        .split_whitespace()
        .last()
        .is_some_and(|word| word.trim_start_matches('&') == "self")
}

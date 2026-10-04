//! The signature assists: `remove_unused_param` and `convert_tuple_return_to_struct`.
//!
//! rust-analyzer offers `remove_unused_param` with the caret on the parameter's name, and
//! `convert_tuple_return_type_to_struct` with the caret inside the return type, while an item anchor
//! names the function. What sits between the two is read out of the source here, so each assist can
//! be asked where it is offered. The module also holds the carets built from those positions, the
//! refusal for a parameter that is still used, and the step that gives the struct the conversion
//! introduces the name the plan asked for.

use super::early_return::masked_to_code;
use super::lsp_edits::{position_at, workspace_edits_for};
use super::{
    apply_lsp_edit, document_changes, edits_in, failure, minimal_edits, relative_path, relative_to,
    seam_refusal, server_defect, uri_of, RustBackend,
};
use crate::edit::{FileEdit, Position, Range, WorkspaceEdit};
use crate::plan::{RefactorKind, RefactorOp};
use crate::registry::Workspace;
use crate::{RestructureError, Result};
use serde_json::{json, Value};

/// The caret for `remove_unused_param`: on the name of the parameter the plan names, inside the
/// function whose own name `range` starts at.
pub(super) fn parameter_caret(
    workspace: &Workspace<'_>,
    op: &RefactorOp,
    range: Range,
) -> Result<Range> {
    let name = op
        .name
        .as_deref()
        .ok_or_else(|| failure("the operation needs a name"))?;
    let text = workspace.read(op.anchor.file())?;
    let at = parameter_position(&text, range.start, name).ok_or_else(|| {
        seam_refusal(format!(
            "`{name}` is not a parameter of the function the anchor names"
        ))
    })?;
    Ok(Range { start: at, end: at })
}

/// The caret for `convert_tuple_return_to_struct`: on the return type of the function whose own
/// name `range` starts at.
pub(super) fn return_type_caret(
    workspace: &Workspace<'_>,
    op: &RefactorOp,
    range: Range,
) -> Result<Range> {
    let text = workspace.read(op.anchor.file())?;
    let at = return_type_position(&text, range.start).ok_or_else(|| {
        seam_refusal("the function the anchor names declares no return type to convert")
    })?;
    Ok(Range { start: at, end: at })
}

/// A used parameter, as the refusal it is: the server offers no removal for a parameter its
/// function still reads, and that absence is the answer rather than an assist to wait for.
///
/// Any other failure — a server that would not start, one still indexing — is returned untouched.
pub(super) fn refuse_used_parameter(
    op: &RefactorOp,
    refusal: RestructureError,
) -> RestructureError {
    match (op.op, op.name.as_deref(), &refusal) {
        (RefactorKind::RemoveUnusedParam, Some(name), RestructureError::SeamRefused(reason)) => {
            seam_refusal(format!(
                "parameter `{name}` is used, so it cannot be removed ({reason})"
            ))
        }
        _ => refusal,
    }
}

/// A document the conversion touched: its path, what is on disk, and what the assist left.
struct Touched {
    path: String,
    original: String,
    updated: String,
}

impl RustBackend {
    /// The edit `convert_tuple_return_type_to_struct` produced, with its struct given the name the
    /// plan asked for.
    ///
    /// The assist names the struct after the function and writes that name into every file it
    /// touches. The rename is asked of the server, against the documents as the assist left them, so
    /// the new name reaches the declaration and every rewritten caller without this backend writing
    /// an identifier itself.
    pub(super) fn name_converted_struct(
        &mut self,
        uri: &str,
        workspace: &Workspace<'_>,
        op: &RefactorOp,
        workspace_edit: &Value,
    ) -> Result<WorkspaceEdit> {
        let name = op
            .name
            .as_deref()
            .ok_or_else(|| failure("the operation needs a name"))?;
        let mut documents = touched_documents(workspace_edit, workspace)?;
        let anchored = op.anchor.file();
        let (introduced, offset) = introduced_in_anchored(&documents, anchored)?;

        if introduced != name {
            self.rename_introduced(uri, workspace, &mut documents, anchored, offset, name)?;
        }

        Ok(WorkspaceEdit {
            changes: documents
                .into_iter()
                .map(|document| FileEdit::Change {
                    edits: minimal_edits(&document.original, &document.updated),
                    path: document.path,
                })
                .collect(),
        })
    }

    /// Opens the documents as the assist left them, asks the server to rename the struct whose name
    /// sits at `offset` in the anchored document, and folds the rename's edits back into
    /// `documents`.
    fn rename_introduced(
        &mut self,
        uri: &str,
        workspace: &Workspace<'_>,
        documents: &mut [Touched],
        anchored: &str,
        offset: usize,
        name: &str,
    ) -> Result<()> {
        let updated = &documents
            .iter()
            .find(|document| document.path == anchored)
            .ok_or_else(|| server_defect("the anchored document vanished"))?
            .updated;
        self.did_change(uri, updated)?;
        for document in documents
            .iter()
            .filter(|document| document.path != anchored)
        {
            let other = uri_of(&workspace.root.join(&document.path));
            self.did_open(&other, &document.updated)?;
        }

        let position = position_at(updated, offset);
        self.wait_until_resolved(uri, &position)?;
        let renamed = self.request_settled(
            "textDocument/rename",
            json!({ "textDocument": { "uri": uri }, "position": position, "newName": name }),
        )?;
        fold_rename(documents, &renamed, workspace)
    }
}

/// Every document the conversion touched, with what the assist left in it.
fn touched_documents(workspace_edit: &Value, workspace: &Workspace<'_>) -> Result<Vec<Touched>> {
    let mut documents = Vec::new();
    for change in document_changes(workspace_edit) {
        if change.get("kind").is_some() {
            return Err(server_defect(
                "rust-analyzer's conversion created a file, which it has no reason to",
            ));
        }
        let path = relative_path(change.pointer("/textDocument/uri"), workspace.root)?;
        let original = workspace.read(&path)?;
        let updated = apply_lsp_edit(&original, edits_in(&change)?);
        documents.push(Touched {
            path,
            original,
            updated,
        });
    }
    Ok(documents)
}

/// The struct the conversion introduced in the anchored document, and the offset of its name.
fn introduced_in_anchored(documents: &[Touched], anchored: &str) -> Result<(String, usize)> {
    let document = documents
        .iter()
        .find(|document| document.path == anchored)
        .ok_or_else(|| {
            server_defect("rust-analyzer's conversion did not edit the function's own file")
        })?;
    introduced_struct(&document.original, &document.updated)
        .ok_or_else(|| server_defect("rust-analyzer's conversion declared no struct to name"))
}

/// Applies the edits of a rename to the documents the conversion touched.
///
/// A rename that reaches a document the conversion did not touch is a server defect, since the
/// struct is new and nothing else can mention it.
fn fold_rename(
    documents: &mut [Touched],
    renamed: &Value,
    workspace: &Workspace<'_>,
) -> Result<()> {
    for (document, edits) in workspace_edits_for(renamed)? {
        let path = relative_to(&document, workspace.root)?;
        let held = documents
            .iter_mut()
            .find(|known| known.path == path)
            .ok_or_else(|| {
                server_defect(format!(
                    "the rename of the introduced struct reached {path}, which the conversion \
                     did not touch"
                ))
            })?;
        held.updated = apply_lsp_edit(&held.updated, edits);
    }
    Ok(())
}

/// The position of the parameter `name` in the parameter list of the function whose own name starts
/// at `function_name`, or `None` when that function has no such parameter.
///
/// A parameter is found by its pattern (`name`, `mut name`), never by a substring, so a type or a
/// default that mentions `name` is not mistaken for it.
pub(super) fn parameter_position(
    text: &str,
    function_name: Position,
    name: &str,
) -> Option<Position> {
    let code = masked_to_code(text);
    let (open, close) = parameter_list(&code, offset_at(text, function_name)?)?;

    let mut depth = 0usize;
    let mut segment_from = open + 1;
    for (index, byte) in code.bytes().enumerate().take(close).skip(open + 1) {
        match byte {
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'>' if !arrow_before(code.as_bytes(), index) => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                if let Some(found) = named_in(&code, segment_from, index, name) {
                    return Some(position_of(text, found));
                }
                segment_from = index + 1;
            }
            _ => {}
        }
    }
    named_in(&code, segment_from, close, name).map(|found| position_of(text, found))
}

/// The position of the return type the function whose own name starts at `function_name` declares,
/// just past its `->`, or `None` when it declares none.
///
/// rust-analyzer offers the tuple-return conversion with the caret inside the return type.
pub(super) fn return_type_position(text: &str, function_name: Position) -> Option<Position> {
    let code = masked_to_code(text);
    let (_, close) = parameter_list(&code, offset_at(text, function_name)?)?;
    let bytes = code.as_bytes();
    let arrow = skip_whitespace(bytes, close + 1);
    if !code[arrow..].starts_with("->") {
        return None;
    }
    let ty = skip_whitespace(bytes, arrow + "->".len());
    (ty < bytes.len()).then(|| position_of(text, ty))
}

/// The offsets of the `(` and `)` around the parameters of the function whose name starts at
/// `name_at`.
pub(super) fn parameter_list(code: &str, name_at: usize) -> Option<(usize, usize)> {
    let bytes = code.as_bytes();
    let after_name = name_at + identifier_length(bytes.get(name_at..)?);
    let open = skip_whitespace(
        bytes,
        skip_generics(bytes, skip_whitespace(bytes, after_name)),
    );
    if bytes.get(open) != Some(&b'(') {
        return None;
    }

    let mut depth = 0usize;
    for (index, byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' => depth = depth.checked_sub(1)?,
            b'>' if !arrow_before(bytes, index) => depth = depth.checked_sub(1)?,
            _ => {}
        }
        if depth == 0 {
            return Some((open, index));
        }
    }
    None
}

/// The tuple struct `updated` declares that `original` did not, and the offset of its name.
///
/// `convert_tuple_return_type_to_struct` names the struct after the function, and what it chose is
/// found by what it added rather than by guessing the spelling. A struct is new when its name is
/// declared more often than before.
pub(super) fn introduced_struct(original: &str, updated: &str) -> Option<(String, usize)> {
    let before = struct_declarations(original);
    let after = struct_declarations(updated);
    let declared = |set: &[(String, usize)], name: &str| {
        set.iter().filter(|(declared, _)| declared == name).count()
    };
    after
        .iter()
        .find(|(name, _)| declared(&after, name) > declared(&before, name))
        .cloned()
}

/// Every `struct Name` in the code of `text`, with the offset of the name.
fn struct_declarations(text: &str) -> Vec<(String, usize)> {
    let code = masked_to_code(text);
    let bytes = code.as_bytes();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = code[from..].find("struct").map(|offset| from + offset) {
        from = at + "struct".len();
        let opens = at == 0 || !(bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
        if !opens || !bytes.get(from).is_some_and(u8::is_ascii_whitespace) {
            continue;
        }
        let name_at = skip_whitespace(bytes, from);
        let length = identifier_length(&bytes[name_at..]);
        if length > 0 {
            found.push((code[name_at..name_at + length].to_string(), name_at));
        }
    }
    found
}

/// Whether the byte at `index` is the `>` of a `->`, which closes nothing.
pub(super) fn arrow_before(bytes: &[u8], index: usize) -> bool {
    bytes[index] == b'>' && index > 0 && bytes[index - 1] == b'-'
}

/// The offset of `name` when the parameter in `code[from..to]` binds exactly it.
fn named_in(code: &str, from: usize, to: usize, name: &str) -> Option<usize> {
    let segment = &code[from..to];
    let pattern_end = segment.find(':')?;
    let pattern = &segment[..pattern_end];
    let trimmed = pattern.trim_start();
    let leading = pattern.len() - trimmed.len();
    let binding = trimmed
        .strip_prefix("mut ")
        .map_or(trimmed, str::trim_start);
    let skipped = trimmed.len() - binding.len();
    (binding.trim_end() == name).then_some(from + leading + skipped)
}

/// The index just past a `<...>` generic list starting at `at`, or `at` when there is none.
fn skip_generics(bytes: &[u8], at: usize) -> usize {
    if bytes.get(at) != Some(&b'<') {
        return at;
    }
    let mut depth = 0usize;
    for index in at..bytes.len() {
        match bytes[index] {
            b'<' => depth += 1,
            b'>' if !arrow_before(bytes, index) => {
                let Some(open) = depth.checked_sub(1) else {
                    return at;
                };
                depth = open;
                if depth == 0 {
                    return index + 1;
                }
            }
            _ => {}
        }
    }
    at
}

pub(super) fn skip_whitespace(bytes: &[u8], from: usize) -> usize {
    from + bytes[from.min(bytes.len())..]
        .iter()
        .take_while(|byte| byte.is_ascii_whitespace())
        .count()
}

fn identifier_length(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'_')
        .count()
}

/// The byte offset of a one-based position.
pub(super) fn offset_at(text: &str, position: Position) -> Option<usize> {
    let line_start = text
        .split_inclusive('\n')
        .take(position.line.checked_sub(1)? as usize)
        .map(str::len)
        .sum::<usize>();
    let offset = line_start + position.col.checked_sub(1)? as usize;
    (offset <= text.len()).then_some(offset)
}

/// The one-based position of a byte offset.
fn position_of(text: &str, offset: usize) -> Position {
    let before = &text[..offset];
    Position {
        line: before.matches('\n').count() as u32 + 1,
        col: before.rsplit('\n').next().map_or(0, str::len) as u32 + 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(line: u32, col: u32) -> Position {
        Position { line, col }
    }

    #[test]
    fn finds_a_parameter_by_its_pattern() {
        let text = "pub fn total(price: u32, mut note: &str) -> u32 {\n    price\n}\n";

        assert_eq!(parameter_position(text, at(1, 8), "note"), Some(at(1, 30)));
    }

    #[test]
    fn reads_past_generics_and_nested_commas_to_the_parameter() {
        let text = "fn f<F: Fn(u32) -> u8>(map: HashMap<u8, u8>, note: u8) {}\n";

        assert_eq!(parameter_position(text, at(1, 4), "note"), Some(at(1, 46)));
    }

    #[test]
    fn finds_the_return_type_after_the_arrow() {
        let text = "pub fn split(total: u32) -> (u32, u32) {\n    (1, 2)\n}\n";

        assert_eq!(return_type_position(text, at(1, 8)), Some(at(1, 29)));
    }

    #[test]
    fn finds_no_return_type_where_none_is_declared() {
        assert_eq!(return_type_position("fn f(a: u8) {}\n", at(1, 4)), None);
    }

    #[test]
    fn finds_no_parameter_list_in_unbalanced_text() {
        assert_eq!(parameter_position("fn f(a: u8 {\n", at(1, 4), "a"), None);
    }

    #[test]
    fn skips_no_generics_past_a_lone_closing_bracket() {
        assert_eq!(skip_generics(b">", 0), 0);
        assert_eq!(skip_generics(b"<>>", 0), 2);
        assert_eq!(skip_generics(b"<u8", 0), 0);
    }

    #[test]
    fn does_not_mistake_a_type_for_a_parameter() {
        let text = "fn f(price: note) {}\n";

        assert_eq!(parameter_position(text, at(1, 4), "note"), None);
    }
}

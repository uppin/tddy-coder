use crate::edit::Range;

use crate::edit::Position;

use super::LspPoint;

use super::server_defect;

use super::LspEdit;

use crate::Result;

use super::UnresolvedName;

use serde_json::{json, Value};

/// Where `wanted` sits in the semantic-token legend the server answered the handshake with.
pub(crate) fn token_type_index(handshake: &Value, wanted: &str) -> Option<u32> {
    handshake
        .pointer("/capabilities/semanticTokensProvider/legend/tokenTypes")?
        .as_array()?
        .iter()
        .position(|entry| entry.as_str() == Some(wanted))
        .map(|index| index as u32)
}

/// Decode a semantic-token response, keeping the tokens of one type.
///
/// The protocol encodes tokens as a flat run of five integers each — line delta, start delta,
/// length, type, modifiers — where the deltas are relative to the token before, and the start
/// delta restarts at the beginning of every new line.
pub(crate) fn unresolved_in(tokens: &Value, wanted: u32, text: &str) -> Vec<UnresolvedName> {
    let lines: Vec<&str> = text.split('\n').collect();
    let data: Vec<u32> = tokens
        .get("data")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(Value::as_u64)
                .map(|n| n as u32)
                .collect()
        })
        .unwrap_or_default();

    let mut found = Vec::new();
    let mut line: u32 = 0;
    let mut character: u32 = 0;

    for entry in data.chunks_exact(5) {
        let [line_delta, start_delta, length, token_type, _] = *entry else {
            continue;
        };
        line += line_delta;
        character = if line_delta == 0 {
            character + start_delta
        } else {
            start_delta
        };

        if token_type != wanted {
            continue;
        }

        // A token whose span does not land inside the document is not one to act on: the server is
        // describing a version of the file this client no longer holds.
        let Some(name) = lines
            .get(line as usize)
            .and_then(|source| source.get(character as usize..(character + length) as usize))
        else {
            continue;
        };

        found.push(UnresolvedName {
            text: name.to_string(),
            position: json!({ "line": line, "character": character }),
        });
    }

    found
}

/// The text edits an LSP workspace edit carries for one document.
///
/// Servers may answer in either shape the protocol allows — `documentChanges` or `changes` — and a
/// bare `WorkspaceEdit` (as `textDocument/rename` returns) or one wrapped in a code action.
pub(crate) fn edits_for(response: &Value, uri: &str) -> Result<Vec<LspEdit>> {
    let workspace_edit = response.get("edit").unwrap_or(response);

    let raw: Vec<Value> = match workspace_edit
        .get("documentChanges")
        .and_then(Value::as_array)
    {
        Some(changes) => changes
            .iter()
            .filter(|change| {
                change.pointer("/textDocument/uri").and_then(Value::as_str) == Some(uri)
            })
            .filter_map(|change| change.get("edits").and_then(Value::as_array))
            .flatten()
            .cloned()
            .collect(),
        None => workspace_edit
            .pointer(&format!("/changes/{}", uri.replace('/', "~1")))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    };

    if raw.is_empty() {
        return Err(server_defect(
            "rust-analyzer returned no edits for the document",
        ));
    }

    raw.iter().map(read_edit).collect()
}

/// Every document a workspace edit touches, with its edits — the multi-document counterpart of
/// [`edits_for`].
///
/// [`edits_for`] filters `documentChanges` down to one `uri`, which is correct for an assist this
/// backend resolves against a single file and **wrong for a rename**: rust-analyzer computes
/// cross-file rename edits, and discarding every document but the anchor's left those callers naming
/// a symbol that no longer existed. The anchor's own file looked right, so nothing surfaced until a
/// later build.
///
/// It is also the primitive `move_module_to_crate` needs, because re-pointing a caller *is* editing
/// another document.
///
/// Documents come back in the order the server listed them, so a caller can report them in a stable
/// order. An edit naming no documents is still an error: this widens what counts as an answer, it
/// does not make silence acceptable, and neither does a document whose edit list is empty.
///
/// Text edits are all it carries. A `documentChanges` entry that is a resource operation — a
/// `create`, `rename` or `delete` — names no `textDocument`, and so is not one of these pairs;
/// [`convert_change`] is where those are read.
pub(crate) fn workspace_edits_for(response: &Value) -> Result<Vec<(String, Vec<LspEdit>)>> {
    let workspace_edit = response.get("edit").unwrap_or(response);

    let documents: Vec<(String, &Vec<Value>)> = match workspace_edit
        .get("documentChanges")
        .and_then(Value::as_array)
    {
        Some(changes) => changes
            .iter()
            .filter_map(|change| {
                let uri = change
                    .pointer("/textDocument/uri")
                    .and_then(Value::as_str)?;
                let edits = change.get("edits").and_then(Value::as_array)?;
                Some((uri.to_string(), edits))
            })
            .filter(|(_, edits)| !edits.is_empty())
            .collect(),
        None => workspace_edit
            .get("changes")
            .and_then(Value::as_object)
            .map(|documents| {
                documents
                    .iter()
                    .filter_map(|(uri, edits)| Some((uri.clone(), edits.as_array()?)))
                    .filter(|(_, edits)| !edits.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
    };

    if documents.is_empty() {
        return Err(server_defect(
            "rust-analyzer returned no edits for any document",
        ));
    }

    documents
        .into_iter()
        .map(|(uri, edits)| Ok((uri, edits.iter().map(read_edit).collect::<Result<_>>()?)))
        .collect()
}

/// One text edit, refusing anything malformed rather than guessing at it.
pub(crate) fn read_edit(entry: &Value) -> Result<LspEdit> {
    Ok(LspEdit {
        start: LspPoint::read(entry.pointer("/range/start"))?,
        end: LspPoint::read(entry.pointer("/range/end"))?,
        new_text: entry
            .get("newText")
            .and_then(Value::as_str)
            .ok_or_else(|| server_defect("edit is missing `newText`"))?
            .to_string(),
    })
}

/// Apply LSP edits to `text`. Ranges share one coordinate space, so they land last-first.
pub(crate) fn apply_lsp_edit(text: &str, mut edits: Vec<LspEdit>) -> String {
    edits.sort_by_key(|edit| edit.start);

    let mut result = text.to_string();
    for edit in edits.into_iter().rev() {
        let from = offset_of(&result, edit.start);
        let to = offset_of(&result, edit.end);
        result.replace_range(from..to.max(from), &edit.new_text);
    }
    result
}

/// Zero-based line/character to a byte offset.
pub(crate) fn offset_of(text: &str, point: LspPoint) -> usize {
    let mut offset = 0usize;
    for _ in 0..point.line {
        match text[offset..].find('\n') {
            None => return text.len(),
            Some(index) => offset += index + 1,
        }
    }
    let end = text[offset..].find('\n').map_or(text.len(), |i| offset + i);

    // `character` is a byte offset into the line, per the encoding declared at initialize, so it is
    // added rather than walked. The boundary walk is for a server that broke that agreement: landing
    // mid-character would panic on the next slice, and advancing to the next boundary is the one
    // recovery that cannot.
    let mut at = offset + point.character.min(end - offset);
    while at < end && !text.is_char_boundary(at) {
        at += 1;
    }
    at
}

/// Byte offset to the zero-based LSP position naming it.
pub(crate) fn position_at(text: &str, offset: usize) -> Value {
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let character = before.rsplit('\n').next().map_or(0, str::len);
    json!({ "line": line, "character": character })
}

/// One position in LSP's zero-based coordinates.
pub(crate) fn lsp_position(at: Position) -> Value {
    json!({ "line": at.line - 1, "character": at.col - 1 })
}

pub(crate) fn lsp_range(range: Range) -> Value {
    json!({
        "start": { "line": range.start.line - 1, "character": range.start.col - 1 },
        "end": { "line": range.end.line - 1, "character": range.end.col - 1 }
    })
}

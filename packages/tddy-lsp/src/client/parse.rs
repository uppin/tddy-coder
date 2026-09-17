//! Reading the wire shapes of the LSP responses this client issues, and writing the one request
//! shape most of them share. Missing or malformed fields read as their zero value: a server is
//! free to omit what it has nothing to say about, and a position it never sent is not an error.

use serde_json::{json, Value};

use super::{Diagnostic, Location, Position, Range};

/// Standard `{ textDocument, position }` request params.
pub(super) fn position_params(uri: &str, pos: Position) -> Value {
    json!({
        "textDocument": { "uri": uri },
        "position": { "line": pos.line, "character": pos.character },
    })
}

pub(super) fn parse_position(value: Option<&Value>) -> Position {
    let value = value.unwrap_or(&Value::Null);
    Position::at(
        value.get("line").and_then(Value::as_u64).unwrap_or(0) as u32,
        value.get("character").and_then(Value::as_u64).unwrap_or(0) as u32,
    )
}

pub(super) fn parse_range(value: Option<&Value>) -> Range {
    let value = value.unwrap_or(&Value::Null);
    Range {
        start: parse_position(value.get("start")),
        end: parse_position(value.get("end")),
    }
}

pub(super) fn parse_location(value: &Value) -> Location {
    Location {
        uri: value
            .get("uri")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        range: parse_range(value.get("range")),
    }
}

pub(super) fn parse_locations(result: &Value) -> Vec<Location> {
    result
        .as_array()
        .map(|items| items.iter().map(parse_location).collect())
        .unwrap_or_default()
}

pub(super) fn parse_diagnostics(value: Option<&Value>) -> Vec<Diagnostic> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| Diagnostic {
                    range: parse_range(item.get("range")),
                    severity: item.get("severity").and_then(Value::as_u64).unwrap_or(0) as u8,
                    message: item
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    source: item
                        .get("source")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                })
                .collect()
        })
        .unwrap_or_default()
}

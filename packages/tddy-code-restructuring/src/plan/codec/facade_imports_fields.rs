//! The refusals specific to `repoint_facade_imports`: a line that carries only an anchor.
//!
//! Every other field of the plan line is one the operation cannot honour, and refusing it is
//! decided from the line alone, before any server is spawned, so plain `check`, `check --deep` and
//! `apply` report it alike. What needs the code (which paths go through a facade, whether the
//! defining crate is declared) is `backends/rust/repoint_facade`'s.
//!
//! The line is read as JSON rather than as a deserialized operation, so a field the operation has
//! no use for is refused *by name* even when its own value would not deserialize — `reexport:
//! "keep"` names `reexport`, not an unknown variant.

use super::super::malformed;
use crate::Result;
use serde_json::Value;

/// The fields a `repoint_facade_imports` line must not carry. Each belongs to another operation.
const REFUSED: [&str; 12] = [
    "to",
    "name",
    "reexport",
    "variant",
    "type",
    "expr",
    "order",
    "also",
    "to_file",
    "with_private_deps",
    "callee",
    "canonical_paths",
];

/// A `repoint_facade_imports` line carries an anchor and nothing else.
pub(super) fn refuse_a_facade_repoint_it_cannot_honour(line: &Value) -> Result<()> {
    if line.get("op").and_then(Value::as_str) != Some("repoint_facade_imports") {
        return Ok(());
    }
    let Some(object) = line.as_object() else {
        return Ok(());
    };
    match REFUSED.iter().find(|field| object.contains_key(**field)) {
        Some(field) => Err(malformed(format!(
            "`{field}` is a field `repoint_facade_imports` cannot honour: it re-points the paths \
             of one file (or of every file of one module) that go through a facade of another \
             crate, and carries only its anchor"
        ))),
        None => Ok(()),
    }
}

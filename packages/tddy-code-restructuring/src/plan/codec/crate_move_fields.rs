//! What a plan may say about a crate a move creates: `name` on `move_module_to_crate` and
//! `move_cluster_to_crate` is the new crate's `[package] name`.
//!
//! Checked over the whole plan rather than one line, because one of the refusals is about two
//! operations: a plan that creates the same crate twice.

use std::collections::BTreeMap;

use crate::plan::{RefactorKind, RefactorOp};
use crate::Result;

use super::malformed;

/// Refuse what no crate move can honour about creating a crate: `name` on
/// `move_test_binary_to_crate`, a `name` that is not a Cargo package name, and two operations that
/// create a crate at the same `to`.
pub(super) fn refuse_crate_creations_it_cannot_honour(ops: &[RefactorOp]) -> Result<()> {
    let mut created_by: BTreeMap<&str, usize> = BTreeMap::new();

    for (index, op) in ops.iter().enumerate() {
        let Some(name) = op.name.as_deref() else {
            continue;
        };
        if op.op == RefactorKind::MoveTestBinaryToCrate {
            return Err(malformed(
                "`move_test_binary_to_crate` moves a test binary into a crate that exists — \
                 `name` creates one only for a module move",
            ));
        }
        if !op.op.moves_across_crates() {
            continue;
        }
        if !is_a_package_name(name) {
            return Err(malformed(format!(
                "`{name}` is not a Cargo package name: it is a non-empty run of ASCII letters, \
                 digits, `-` and `_` that does not start with a digit or `-`, and is not `test`"
            )));
        }
        // `moves_across_crates` operations are refused without `to` on their own line.
        let Some(to) = op.to.as_deref() else {
            continue;
        };
        if let Some(earlier) = created_by.insert(to, index) {
            return Err(malformed(format!(
                "`{to}` is created by operation {earlier} already — a plan creates a crate once \
                 and moves into it afterwards without `name`"
            )));
        }
    }
    Ok(())
}

/// Whether `name` is a `[package] name` cargo accepts and a `use` path can spell once its hyphens
/// are underscores.
fn is_a_package_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        && name != "test"
}

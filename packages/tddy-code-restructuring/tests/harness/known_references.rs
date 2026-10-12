//! A reference set standing in for `textDocument/references`, for the server-free crate-move suites.
//!
//! The cross-crate moves ask a [`ModuleReferences`] engine which places outside a file name each item
//! the file declares. Production asks rust-analyzer; a library-level test hands the move a known
//! answer instead, so it decides what the move writes in milliseconds and without a server.

use std::collections::BTreeMap;

use tddy_code_restructuring::crate_move::{
    DeclarationKind, ItemReferences, ModuleReferences, Reference,
};
use tddy_code_restructuring::registry::Workspace;
use tddy_code_restructuring::{Position, RestructureError};

/// Which places outside a file name each item it declares.
#[derive(Default)]
pub struct AKnownReferenceSet {
    by_file: BTreeMap<String, Vec<ItemReferences>>,
}

/// A reference set in which nothing outside any file names anything.
pub fn nothing_reaches_anything() -> AKnownReferenceSet {
    AKnownReferenceSet::default()
}

impl AKnownReferenceSet {
    /// Every place the file `from`, whose text is `from_text`, writes `item`, which the file
    /// `declared_in` declares.
    pub fn reaching(mut self, item: &str, declared_in: &str, from: &str, from_text: &str) -> Self {
        let referenced_at = from_text
            .match_indices(item)
            .map(|(offset, _)| Reference {
                path: from.to_string(),
                at: position_of(from_text, offset),
            })
            .collect();
        self.by_file
            .entry(declared_in.to_string())
            .or_default()
            .push(ItemReferences {
                item: item.to_string(),
                referenced_at,
                // A top-level item at the file's start: these suites are not about widening.
                declared_at: Position { line: 1, col: 1 },
                within: Vec::new(),
                kind: DeclarationKind::Item,
            });
        self
    }
}

impl ModuleReferences for AKnownReferenceSet {
    fn outside_references(
        &mut self,
        _workspace: &Workspace<'_>,
        file: &str,
    ) -> Result<Vec<ItemReferences>, RestructureError> {
        Ok(self.by_file.get(file).cloned().unwrap_or_default())
    }
}

/// The one-based position of a byte offset, as the server reports a reference.
pub fn position_of(text: &str, offset: usize) -> Position {
    let before = &text[..offset];
    Position {
        line: before.matches('\n').count() as u32 + 1,
        col: before
            .rsplit('\n')
            .next()
            .map_or(0, |line| line.chars().count()) as u32
            + 1,
    }
}

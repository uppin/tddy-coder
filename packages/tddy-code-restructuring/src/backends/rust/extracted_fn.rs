//! What `extract_method` does to the function rust-analyzer's "extract into function" wrote, before
//! the edit is produced.
//!
//! The assist's output compiles and still leaves CI's lint job red, loses the comments that say
//! *why*, and names types the file does not import. Every rule here acts on **the introduced
//! function only** — the `fn <name>` the rename produced, from its keyword to its closing brace —
//! and on the call line the assist wrote, never on the rest of the file. See the changeset
//! `docs/dev/1-WIP/2026-10-09-reshape-extract-method-clean.md` § The rules (rules 1-14).
//!
//! Kept out of `backends/rust.rs`, which is past its size budget: that file only wires this in.

mod comments;
mod guards;
mod lints;
mod ptr_args;
mod respell;
mod return_type;
mod span;

use super::{ProgressSink, RustBackend};
use crate::edit::Range;
use crate::Result;

/// The text of the file after the clean-up, and what the engine has to say about it — a parameter
/// it kept as written, a parameter count clippy will flag.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Cleaned {
    pub text: String,
    pub notes: Vec<String>,
}

impl RustBackend {
    /// The file with the function `name` that the assist introduced cleaned up: the function-local
    /// `use` carry, then the guard lift (rule 12), the comment carry (rule 1), the respelling of an
    /// unimported signature type (rule 6), the `ptr_arg` narrowing (rule 5), the lexical clean-ups
    /// (rules 2-4) and the caller's return-type spelling (rule 14), in that order.
    pub(super) fn cleaned_extraction(
        &mut self,
        uri: &str,
        original: &str,
        named: &str,
        range: Range,
        name: &str,
    ) -> Result<Cleaned> {
        let _ = (uri, original, named, range, name);
        // TODO(reshape-extract-method-clean): implement
        todo!("cleaned_extraction")
    }

    /// `text` with each `&PathBuf` / `&String` / `&Vec<T>` parameter of `name` narrowed, keeping
    /// only the narrowings rust-analyzer's pull diagnostics raise no new `E0308` for (rule 5). A
    /// narrowing undone is a note.
    pub(super) fn verified_narrowings(
        &mut self,
        uri: &str,
        text: &str,
        name: &str,
    ) -> Result<Cleaned> {
        let _ = (uri, text, name);
        // TODO(reshape-extract-method-clean): implement
        todo!("verified_narrowings")
    }

    /// `text` with every type `name`'s signature names that the server reports unresolved — and
    /// that resolved before the assist — spelled the way the function the range came from spells
    /// it (rule 6). Zero or several spellings refuse the operation, naming the type.
    pub(super) fn respelled_signature_types(
        &mut self,
        uri: &str,
        original: &str,
        text: &str,
        range: Range,
        name: &str,
    ) -> Result<String> {
        let _ = (uri, original, text, range, name);
        // TODO(reshape-extract-method-clean): implement
        todo!("respelled_signature_types")
    }
}

/// What `check` says about an `extract_method` range before any server exists: the early-exit
/// refusal (or nothing, for a range the guard lift or tail position accepts), the function-local
/// `use` items the extraction will carry and the comments it will put back, each on the progress
/// line. Replaces the `ExtractMethod` block of `RustBackend::check`.
pub(super) fn extract_method_findings(
    text: &str,
    range: Range,
    progress: &ProgressSink,
) -> Vec<String> {
    let _ = (text, range, progress);
    // TODO(reshape-extract-method-clean): implement
    todo!("extract_method_findings")
}

/// Refuse an `extract_method` range holding a `return` that neither the guard lift (rule 12) nor
/// tail position (rule 13) can carry. Replaces `refuse_early_returns` at its two call sites, one
/// line for one line.
pub(super) fn refuse_unliftable_returns(text: &str, range: Range) -> Result<()> {
    let _ = (text, range);
    // TODO(reshape-extract-method-clean): implement
    todo!("refuse_unliftable_returns")
}

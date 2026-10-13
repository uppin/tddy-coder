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
mod signature;
mod span;
mod tail;

use super::early_return::{byte_offset, masked_to_code};
use super::lsp_edits::offset_of;
use super::{imports, seam_refusal, server_defect, LspPoint, ProgressSink, RustBackend};
use crate::edit::Range;
use crate::Result;
use ptr_args::Narrowing;
use serde_json::json;
use std::ops::Range as Bytes;

/// The text of the file after the clean-up, and what the engine has to say about it — a parameter
/// it kept as written, a parameter count clippy will flag.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Cleaned {
    pub text: String,
    pub notes: Vec<String>,
}

impl Cleaned {
    /// `text` as it is, with nothing to say about it.
    pub(super) fn unchanged(text: String) -> Self {
        Self {
            text,
            notes: Vec::new(),
        }
    }
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
        let carried = imports::carry_function_local_uses(original, named, range, name);
        let header = origin_header(original, range);
        let exits = guards::classify(original, range);
        let lifted = guards::lifted(&header, &carried, name, &exits)?;
        let (commented, _) = comments::restored(original, range, &lifted, name)?;
        let respelled = self.respelled_signature_types(uri, original, &commented, range, name)?;
        let narrowed = self.verified_narrowings(uri, &respelled, name)?;
        let unwrapped = lints::unit_tail_unwrapped(&narrowed.text, name);
        let shorthand = lints::field_shorthand(&unwrapped, name);
        let mut notes = narrowed.notes;
        notes.extend(lints::shape_notes(&shorthand, name));
        Ok(Cleaned {
            text: return_type::respelled_return_type(&header, &shorthand, name),
            notes,
        })
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
        let chosen = ptr_args::narrowings(text, name);
        if chosen.is_empty() {
            return Ok(Cleaned::unchanged(text.to_string()));
        }
        let all = ptr_args::narrowed(text, name, &chosen);
        let rejected = self.mismatches_in(uri, &all, name)?;
        if rejected.is_empty() {
            return Ok(Cleaned::unchanged(all));
        }

        // Undo the narrowings whose parameter the rejected lines name; when none is named, the
        // answer cannot say which is wrong, so all are.
        let mut undone = ptr_args::blamed(&chosen, &all, &rejected);
        if undone.is_empty() {
            undone = chosen.iter().collect();
        }
        let kept: Vec<Narrowing> = chosen
            .iter()
            .filter(|narrowing| !undone.contains(narrowing))
            .cloned()
            .collect();
        let retried = ptr_args::narrowed(text, name, &kept);
        let sound = kept.is_empty() || self.mismatches_in(uri, &retried, name)?.is_empty();
        let (text, undone) = if sound {
            (retried, undone)
        } else {
            (text.to_string(), chosen.iter().collect())
        };
        Ok(Cleaned {
            text,
            notes: undone.into_iter().map(ptr_args::kept_note).collect(),
        })
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
        let Some(span) = span::function_span(text, name) else {
            return Ok(text.to_string());
        };
        let signature = span.keyword..span.open;
        let unresolved = self.unresolved_within(uri, text, signature)?;
        if unresolved.is_empty() {
            return Ok(text.to_string());
        }
        let before = self.unresolved_within(uri, original, 0..original.len())?;
        let new: Vec<(String, usize)> = unresolved
            .into_iter()
            .filter(|(name, _)| !before.iter().any(|(known, _)| known == name))
            .collect();
        let masked_original = masked_to_code(original);
        let origin = byte_offset(original, range.start)
            .and_then(|from| span::enclosing_function(masked_original.as_bytes(), from))
            .map_or("", |origin| &original[origin.keyword..=origin.close]);

        let respelled = respell::respelled(text, &new, origin)?;
        let after = span::function_span(&respelled, name).map_or(0..0, |s| s.keyword..s.open);
        let still = self.unresolved_within(uri, &respelled, after)?;
        for (spelled, _) in &new {
            if still.iter().any(|(left, _)| left == spelled) {
                return Err(server_defect(format!(
                    "rust-analyzer still cannot resolve `{spelled}` in `{name}`'s signature once it \
                     is spelled the way the function it came from spells it"
                )));
            }
        }
        Ok(respelled)
    }

    /// The names the server cannot resolve in `text` within bytes `within`, with their offsets.
    fn unresolved_within(
        &mut self,
        uri: &str,
        text: &str,
        within: Bytes<usize>,
    ) -> Result<Vec<(String, usize)>> {
        self.did_change(uri, text)?;
        let mut found = Vec::new();
        for unresolved in self.unresolved_names(uri, text)? {
            let at = offset_of(text, LspPoint::read(Some(&unresolved.position))?);
            if within.contains(&at) {
                found.push((unresolved.text, at));
            }
        }
        Ok(found)
    }

    /// The zero-based lines of `text` — which the server is shown — that it raises an `E0308` on,
    /// inside `name`'s function or on the line calling it.
    fn mismatches_in(&mut self, uri: &str, text: &str, name: &str) -> Result<Vec<usize>> {
        self.did_change(uri, text)?;
        let report = self.request_settled(
            "textDocument/diagnostic",
            json!({ "textDocument": { "uri": uri } }),
        )?;
        Ok(ptr_args::mismatch_lines(&report, text, name))
    }
}

/// The signature of the function `range` lies in, from its `fn` to the `{` opening its body.
fn origin_header(original: &str, range: Range) -> String {
    let masked = masked_to_code(original);
    byte_offset(original, range.start)
        .and_then(|from| span::enclosing_function(masked.as_bytes(), from))
        .map(|origin| original[origin.keyword..=origin.open].to_string())
        .unwrap_or_default()
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
    let refusal = refuse_unliftable_returns(text, range).err();
    // Not findings: what `apply` does about them, so the plan is sound.
    for statement in imports::local_uses_to_carry(text, range) {
        progress(&format!(
            "the extracted function will carry the function-local `{statement}`"
        ));
    }
    let at_risk = comments::at_risk(text, range);
    if at_risk > 0 {
        progress(&format!(
            "the extracted function will keep {at_risk} comment(s) rust-analyzer drops"
        ));
    }
    refusal
        .map(|refusal| refusal.to_string())
        .into_iter()
        .collect()
}

/// Refuse an `extract_method` range holding a `return` that neither the guard lift (rule 12) nor
/// tail position (rule 13) can carry. Replaces `refuse_early_returns` at its two call sites, one
/// line for one line.
pub(super) fn refuse_unliftable_returns(text: &str, range: Range) -> Result<()> {
    match guards::classify(text, range) {
        guards::Exits::Refused(refusal) => Err(seam_refusal(refusal)),
        _ => Ok(()),
    }
}

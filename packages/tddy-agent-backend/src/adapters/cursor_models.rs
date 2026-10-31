//! `cursor-models`: the catalogue `agent --list-models` prints (`#agent-tools 9/12`), read by the
//! parser that already reads it (`backend::cursor::parse_cursor_model_list`) — wrapped, not
//! rewritten. A launch adapter ([`super::LaunchAdapters`]), as `opencode-models` is.

use super::launch::ModelsAdapter;
use crate::backend::BackendModels;

/// [`CursorModels`]'s name.
pub const CURSOR_MODELS: &str = "cursor-models";

/// Reads `agent --list-models`: `<id> - <label>` lines, the header and blanks skipped, the entry
/// tagged `(current, default)` the default (else the first).
#[derive(Debug, Clone, Copy, Default)]
pub struct CursorModels;

impl ModelsAdapter for CursorModels {
    fn name(&self) -> &str {
        CURSOR_MODELS
    }

    fn parse_models(&self, stdout: &str) -> BackendModels {
        // TODO(headless-others): implement — `backend::cursor::parse_cursor_model_list`, which
        // moves here when `CursorBackend::list_models_sync` reads through the spec.
        let _ = stdout;
        todo!("headless-others: CursorModels::parse_models")
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::backend::cursor::parse_cursor_model_list;

    /// `agent --list-models` as it prints it: a header, a blank, then one model per line.
    const RECORDED_LISTING: &str = "Available models\n\nauto - Auto\ncomposer-2.5 - Composer 2.5 (current, default)\ngpt-5.3-codex - GPT-5.3 Codex\n";

    #[test]
    fn a_recorded_listing_reads_into_todays_catalogue() {
        // Given
        let today = parse_cursor_model_list(RECORDED_LISTING);

        // When
        let models = CursorModels.parse_models(RECORDED_LISTING);

        // Then
        assert_eq!(models, today);
    }

    #[test]
    fn todays_parser_reads_the_recorded_listing_with_its_tagged_default() {
        // Given — the positive control for the equality above

        // When
        let today = parse_cursor_model_list(RECORDED_LISTING);

        // Then
        assert_eq!(
            (today.models.len(), today.default_model.as_str()),
            (3, "composer-2.5")
        );
    }
}

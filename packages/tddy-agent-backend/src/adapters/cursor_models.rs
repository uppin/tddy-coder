//! `cursor-models`: the catalogue `agent --list-models` prints (`#agent-tools 9/12`), read by the
//! parser that already reads it (`backend::cursor::parse_cursor_model_list`) — wrapped, not
//! rewritten. A launch adapter ([`super::LaunchAdapters`]), as `opencode-models` is.

use super::launch::ModelsAdapter;
use crate::backend::{BackendModel, BackendModels};

/// Parse the stdout of `agent --list-models` into a [`BackendModels`] catalog.
///
/// Each catalog line is `"<id> - <label>"`; a header line and blanks are ignored, and the entry
/// tagged `"(current, default)"` becomes `default_model` with the marker stripped from its label.
pub(crate) fn parse_cursor_model_list(stdout: &str) -> BackendModels {
    const DEFAULT_MARKER: &str = "(current, default)";
    let mut models = Vec::new();
    let mut default_model: Option<String> = None;
    for line in stdout.lines() {
        let line = line.trim();
        let Some((id, label)) = line.split_once(" - ") else {
            // Header ("Available models") and blank lines carry no " - " separator.
            continue;
        };
        let id = id.trim().to_string();
        let label = label.trim();
        let (label, is_default) = match label.strip_suffix(DEFAULT_MARKER) {
            Some(stripped) => (stripped.trim_end().to_string(), true),
            None => (label.to_string(), false),
        };
        if is_default {
            default_model = Some(id.clone());
        }
        models.push(BackendModel::new(id, label));
    }
    let default_model = default_model
        .or_else(|| models.first().map(|m| m.id.clone()))
        .unwrap_or_default();
    BackendModels {
        models,
        default_model,
    }
}

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
        parse_cursor_model_list(stdout)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

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

    const LIST_MODELS_STDOUT: &str = "Available models\n\
        \n\
        auto - Auto\n\
        gpt-5.2 - GPT-5.2\n\
        composer-2.5 - Composer 2.5 (current, default)\n";

    #[test]
    fn parses_list_models_output_into_id_and_label_pairs() {
        // When
        let catalog = parse_cursor_model_list(LIST_MODELS_STDOUT);

        // Then — the header/blank lines are ignored and each entry is split on " - "
        let pairs: Vec<(&str, &str)> = catalog
            .models
            .iter()
            .map(|m| (m.id.as_str(), m.label.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("auto", "Auto"),
                ("gpt-5.2", "GPT-5.2"),
                ("composer-2.5", "Composer 2.5"),
            ]
        );
    }

    #[test]
    fn treats_the_current_default_entry_as_the_default_model() {
        // When
        let catalog = parse_cursor_model_list(LIST_MODELS_STDOUT);

        // Then — the "(current, default)" marker selects the default and is stripped from the label
        assert_eq!(catalog.default_model, "composer-2.5");
    }
}

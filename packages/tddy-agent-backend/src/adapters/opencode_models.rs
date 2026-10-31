//! `opencode-models`: the catalogue `opencode models` prints — one `provider/model` per line, the
//! first the default (`#agent-tools 8/12`). Named by `opencode.yaml`'s `coding_agent.models`.

use super::launch::ModelsAdapter;
use crate::backend::BackendModels;

/// [`OpenCodeModels`]'s name.
pub const OPENCODE_MODELS: &str = "opencode-models";

/// Reads `opencode models`: blank lines skipped, surrounding whitespace trimmed, each id its own
/// label, the first one the default — what `cli_harness::models_from_listing` reads today.
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenCodeModels;

impl ModelsAdapter for OpenCodeModels {
    fn name(&self) -> &str {
        OPENCODE_MODELS
    }

    fn parse_models(&self, stdout: &str) -> BackendModels {
        // TODO(harness-clis): implement — `cli_harness::models_from_listing` moves here when the
        // `CliHarness` table is deleted.
        let _ = stdout;
        todo!("harness-clis: OpenCodeModels::parse_models")
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::backend::BackendModel;

    /// `opencode models` as v1.0 prints it: one id per line, a trailing blank line.
    const RECORDED_LISTING: &str =
        "anthropic/claude-sonnet-4-5\n  openai/gpt-5.1-codex  \n\nopencode/grok-code\n\n";

    #[test]
    fn a_recorded_listing_is_the_catalogue_with_its_first_model_the_default() {
        // When
        let models = OpenCodeModels.parse_models(RECORDED_LISTING);

        // Then
        assert_eq!(
            models,
            BackendModels {
                models: vec![
                    BackendModel::new("anthropic/claude-sonnet-4-5", "anthropic/claude-sonnet-4-5"),
                    BackendModel::new("openai/gpt-5.1-codex", "openai/gpt-5.1-codex"),
                    BackendModel::new("opencode/grok-code", "opencode/grok-code"),
                ],
                default_model: "anthropic/claude-sonnet-4-5".to_string(),
            }
        );
    }

    #[test]
    fn an_empty_listing_is_an_empty_catalogue_with_no_default() {
        // When
        let models = OpenCodeModels.parse_models("\n  \n");

        // Then
        assert_eq!(
            models,
            BackendModels {
                models: Vec::new(),
                default_model: String::new(),
            }
        );
    }
}

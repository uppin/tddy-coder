//! Launch adapters: the Rust code an interactive (PTY) coding agent's launch reads its *own*
//! output through, by name (`#agent-tools 8/12`).
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-harness-clis-from-specs.md`.
//!
//! A [`super::StreamAdapter`] reads a headless turn's stdout as it streams. These read something
//! else, after the fact: the stdout of a model-listing command (`opencode models`), and a PTY
//! session's terminal output once the agent exits (Warp's `warp --resume <TOKEN>` banner). Their
//! shapes share nothing with a stream, so they are their own small traits and their own registry
//! rather than more methods on [`super::AdapterRegistry`]. A spec only **names** one
//! (`coding_agent.models.adapter`, `coding_agent.resume.adapter`), and a name [`LaunchAdapters`]
//! does not hold is refused when the spec loads.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::backend::BackendModels;

/// How an agent's model-listing command's stdout becomes its model catalogue.
pub trait ModelsAdapter: Send + Sync + fmt::Debug {
    /// The name a spec's `coding_agent.models.adapter` names it by.
    fn name(&self) -> &str;

    /// The catalogue `stdout` lists, its default first.
    fn parse_models(&self, stdout: &str) -> BackendModels;
}

/// How a PTY session's terminal output, once its agent exited, yields the token its next launch
/// resumes with.
pub trait ResumeTokenAdapter: Send + Sync + fmt::Debug {
    /// The name a spec's `coding_agent.resume.adapter` names it by.
    fn name(&self) -> &str;

    /// The token `output` — a terminal's raw bytes, escape sequences included — last printed;
    /// `None` when the run printed none.
    fn resume_token(&self, output: &[u8]) -> Option<String>;
}

/// The launch adapters this build has, by name.
#[derive(Clone, Default)]
pub struct LaunchAdapters {
    models: BTreeMap<String, Arc<dyn ModelsAdapter>>,
    resume_tokens: BTreeMap<String, Arc<dyn ResumeTokenAdapter>>,
}

impl fmt::Debug for LaunchAdapters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LaunchAdapters")
            .field("models", &self.models.keys().collect::<Vec<_>>())
            .field(
                "resume_tokens",
                &self.resume_tokens.keys().collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl LaunchAdapters {
    /// Every launch adapter this build registers: [`super::OPENCODE_MODELS`]
    /// ([`super::OpenCodeModels`]) and [`super::WARP_RESUME_TOKEN`] ([`super::WarpResumeToken`]).
    pub fn builtin() -> Self {
        // TODO(harness-clis): implement — register `OpenCodeModels` and `WarpResumeToken` under
        // their `name()`.
        todo!("harness-clis: LaunchAdapters::builtin")
    }

    /// The models adapter registered as `name`.
    pub fn models(&self, name: &str) -> Option<Arc<dyn ModelsAdapter>> {
        self.models.get(name).cloned()
    }

    /// The resume-token adapter registered as `name`.
    pub fn resume_token(&self, name: &str) -> Option<Arc<dyn ResumeTokenAdapter>> {
        self.resume_tokens.get(name).cloned()
    }

    /// The registered models adapters' names, in order.
    pub fn models_names(&self) -> impl Iterator<Item = &str> {
        self.models.keys().map(String::as_str)
    }

    /// The registered resume-token adapters' names, in order.
    pub fn resume_token_names(&self) -> impl Iterator<Item = &str> {
        self.resume_tokens.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::super::{OPENCODE_MODELS, WARP_RESUME_TOKEN};
    use super::*;

    #[test]
    fn the_builtin_launch_adapters_are_opencode_models_and_warp_resume_token() {
        // Given
        let adapters = LaunchAdapters::builtin();

        // When
        let names = (
            adapters.models_names().collect::<Vec<_>>(),
            adapters.resume_token_names().collect::<Vec<_>>(),
        );

        // Then
        assert_eq!(names, (vec![OPENCODE_MODELS], vec![WARP_RESUME_TOKEN]));
    }

    #[test]
    fn each_builtin_launch_adapter_answers_to_its_name_and_an_unregistered_name_is_absent() {
        // Given
        let adapters = LaunchAdapters::builtin();

        // When
        let found = (
            adapters
                .models(OPENCODE_MODELS)
                .map(|adapter| adapter.name().to_string()),
            adapters
                .resume_token(WARP_RESUME_TOKEN)
                .map(|adapter| adapter.name().to_string()),
            adapters.models("muse-models").is_none(),
            adapters.resume_token(OPENCODE_MODELS).is_none(),
        );

        // Then
        assert_eq!(
            found,
            (
                Some(OPENCODE_MODELS.to_string()),
                Some(WARP_RESUME_TOKEN.to_string()),
                true,
                true,
            )
        );
    }
}

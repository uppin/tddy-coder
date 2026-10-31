//! Stream adapters: the Rust code a declared coding agent's output is read through, by name.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-headless-agents-from-specs.md` (`#agent-tools 6/12`).
//!
//! A coding agent's tool file (`tddy-coding-agent-specs`) declares *how it is launched*; how its
//! output is *read* is a runtime protocol that cannot be YAML. So a spec's `coding_agent.headless`
//! section only **names** an adapter (`adapter: claude-stream-json`), and the [`AdapterRegistry`]
//! maps that name to the parser that already reads the agent's stream. A spec naming a name the
//! registry does not hold is refused when it loads.
//!
//! The registry lives here, beside the parsers it wraps, and not in the specs crate: the specs crate
//! depends on this one, so it validates names against [`AdapterRegistry::builtin`] directly and no
//! dependency points back.
//!
//! [`launch`] holds the PTY-side adapters (`#agent-tools 8/12`): `opencode-models` reads a model
//! listing and `warp-resume-token` scrapes a resume token off an exited session's terminal. Their
//! shapes are not a stream's, so they have their own traits and registry ([`LaunchAdapters`]).
//!
//! [`headless`] is the other half of the seam: the port a spec-driven renderer implements, so the
//! consumers in this crate (`ClaudeCodeBackend`, `HeadlessDriver`) can launch from a spec without
//! naming the specs crate.

mod claude_stream_json;
mod codex_jsonl;
mod cursor_models;
mod cursor_stream;
pub mod headless;
pub mod launch;
mod opencode_models;
mod oz_ndjson;
mod warp_resume_token;

use std::collections::BTreeMap;
use std::fmt;
use std::io::BufRead;
use std::sync::Arc;

use crate::backend::headless_driver::HeadlessHarness;
use crate::error::BackendError;
use crate::stream::{ProgressEvent, StreamResult};

/// The error a [`HeadlessRenderer`] and a [`StreamAdapter`] report, named here for implementors
/// outside this crate.
pub use crate::error::BackendError as AdapterError;
pub use claude_stream_json::ClaudeStreamJson;
// `#agent-tools 9/12`: the stream adapters of Cursor's, Codex's and Oz's headless runs, and Cursor's
// model listing.
pub use codex_jsonl::{CodexJsonl, CODEX_JSONL};
pub use cursor_models::{CursorModels, CURSOR_MODELS};
pub use cursor_stream::{CursorStream, CURSOR_STREAM};
pub use headless::{HeadlessInvocation, HeadlessMcpServer, HeadlessRenderer, HeadlessTurn};
pub use oz_ndjson::{OzNdjson, OZ_NDJSON};
// `#agent-tools 8/12`: what a PTY launch reads its agent's own output through — a model listing,
// a resume token. Not stream adapters, so a registry of their own ([`launch::LaunchAdapters`]).
pub use launch::{LaunchAdapters, ModelsAdapter, ResumeTokenAdapter};
pub use opencode_models::{OpenCodeModels, OPENCODE_MODELS};
pub use warp_resume_token::{WarpResumeToken, WARP_RESUME_TOKEN};

/// The adapter that reads Claude Code's `--output-format stream-json --verbose`.
pub const CLAUDE_STREAM_JSON: &str = "claude-stream-json";

/// Where an adapter reports what it reads as the stream arrives. The callbacks of today's
/// `stream::process_ndjson_stream`, named.
pub struct StreamSinks<'a> {
    /// Each tool-activity or task event.
    pub on_progress: &'a mut dyn FnMut(&ProgressEvent),
    /// The agent's human-readable output, as it streams.
    pub on_raw_output: &'a mut dyn FnMut(&str),
    /// Every raw line, for debug logging; `None` logs nothing.
    pub on_debug_line: Option<&'a mut dyn FnMut(&str)>,
    /// Every raw line, for the conversation log; `None` keeps none.
    pub on_conversation_line: Option<&'a mut dyn FnMut(&str)>,
}

/// One agent's output protocol: how a child's stdout becomes the turn's result, and which exits
/// count as success.
pub trait StreamAdapter: Send + Sync + fmt::Debug {
    /// The name a spec's `adapter:` key names it by.
    fn name(&self) -> &str;

    /// Reads `stdout` to its end, reporting into `sinks` as it goes: the turn's result text, its
    /// session id, the questions it asked, its raw lines and its stream errors.
    fn read_stream(
        &self,
        stdout: &mut dyn BufRead,
        sinks: StreamSinks<'_>,
    ) -> Result<StreamResult, BackendError>;

    /// Whether a child that exited with `exit_code` after streaming `stream` succeeded. With
    /// `exit_ok_if_structured` (the spec's `coding_agent.headless.exit_ok_if_structured`), a
    /// non-zero exit whose result carries a structured response still counts as success.
    fn exit_ok(&self, exit_code: i32, stream: &StreamResult, exit_ok_if_structured: bool) -> bool;

    /// Which of the headless driver's turn transcripts decodes this stream into conversation
    /// updates; `None` for an adapter no conversation driver reads.
    fn turn_format(&self) -> Option<HeadlessHarness>;
}

/// The stream adapters this build has, by name.
#[derive(Clone, Default)]
pub struct AdapterRegistry {
    adapters: BTreeMap<String, Arc<dyn StreamAdapter>>,
}

impl fmt::Debug for AdapterRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.adapters.keys()).finish()
    }
}

impl AdapterRegistry {
    /// Every adapter this build registers: [`CLAUDE_STREAM_JSON`] ([`ClaudeStreamJson`]).
    /// `#agent-tools 9/12` adds Cursor's, Codex's and Oz's.
    pub fn builtin() -> Self {
        // TODO(claude-headless): implement — register `ClaudeStreamJson` under its `name()`.
        // TODO(headless-others): and `CursorStream`, `CodexJsonl` and `OzNdjson` — which widens
        // `the_builtin_registry_lists_claude_stream_json` below at #798's green.
        todo!("claude-headless: AdapterRegistry::builtin")
    }

    /// The adapter registered as `name`.
    pub fn get(&self, name: &str) -> Option<Arc<dyn StreamAdapter>> {
        self.adapters.get(name).cloned()
    }

    /// Whether an adapter is registered as `name`.
    pub fn contains(&self, name: &str) -> bool {
        self.adapters.contains_key(name)
    }

    /// The registered names, in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.adapters.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn the_builtin_registry_lists_claude_stream_json() {
        // Given
        let registry = AdapterRegistry::builtin();

        // When
        let names: Vec<&str> = registry.names().collect();

        // Then
        assert_eq!(names, vec![CLAUDE_STREAM_JSON]);
    }

    #[test]
    fn the_builtin_claude_stream_json_adapter_answers_to_its_name() {
        // Given
        let registry = AdapterRegistry::builtin();

        // When
        let adapter = registry.get(CLAUDE_STREAM_JSON);

        // Then
        assert_eq!(
            adapter.map(|adapter| adapter.name().to_string()),
            Some(CLAUDE_STREAM_JSON.to_string())
        );
    }

    #[test]
    fn a_name_no_adapter_is_registered_as_is_absent_while_claude_stream_json_is_present() {
        // Given
        let registry = AdapterRegistry::builtin();

        // When
        let present = (
            registry.contains(CLAUDE_STREAM_JSON),
            registry.contains("claude-ndjson-v2"),
        );

        // Then
        assert_eq!(present, (true, false));
    }
}

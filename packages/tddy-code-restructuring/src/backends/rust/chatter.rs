//! What rust-analyzer says about its own progress, folded into one account.
//!
//! Split out of `backends/rust.rs`, which is past its size budget.

use std::collections::HashMap;

use serde_json::Value;

/// What a progress line calls a phase the server never gave a title.
const UNTITLED_PHASE: &str = "working";

/// What the server has said about its own progress while a request was in flight.
///
/// `request` used to drop every message it was not waiting for, which is why a run that spent two
/// minutes loading a crate graph showed nothing at all and then failed claiming the plan was
/// malformed. Folding those messages in costs one match per message and turns the wait into
/// something a developer can watch.
///
/// **Published because the fold is not this backend's alone.** `tddy-index-daemon` observes the
/// same two notifications — `$/progress` and `experimental/serverStatus` — to answer "is this
/// root's graph loaded?", and there is exactly one right way to read them: a title arrives only
/// with `begin` and has to be carried forward per token, the same phase reports once per file
/// scanned, and the furthest percentage is not the last one. A second reading of that would drift
/// from this one, and the two would then disagree about a load they were both watching.
#[derive(Default)]
pub struct ServerChatter {
    /// The title of each work-done progress token in flight, by token. A title arrives only with
    /// `begin`, so it has to be carried forward to the `report` lines that follow — and per token,
    /// because rust-analyzer runs several phases at once and a single field would attribute one
    /// phase's reports to another's title.
    titles: HashMap<String, String>,
    /// The last line built, which is what the timeout message needs in order to say where the
    /// server got to.
    pub(super) last: Option<String>,
    /// What was last printed for each token, deduplicated per token for the same reason the titles
    /// are: two phases running at once would otherwise each defeat the other's deduplication.
    shown: HashMap<String, String>,
    /// Whether the server has reported itself quiescent — an extension, so never the only signal.
    pub(super) quiescent: bool,
    /// Whether the server has sent any `experimental/serverStatus` at all.
    ///
    /// What turns a `false` in `quiescent` from "has not said" into "has said it is still
    /// loading". Only the second is a reason to wait: a server that never sends the extension must
    /// still be usable. A shared client keeps the server's latest status for every reader
    /// (`tddy_lsp`'s `LspClient::server_status`), so a transition another reader drained still
    /// arrives here through the bridge.
    reported_status: bool,
    /// What rust-analyzer last said about its own health (`ok`, `warning` or `error`), and the
    /// message it gave for it, from the most recent `experimental/serverStatus`.
    ///
    /// Recorded because a quiescent server is not a trustworthy one. rust-analyzer finishes loading
    /// — and says `quiescent: true` — even when a build script failed, and from then on it answers
    /// every request as though the code that build script should have generated did not exist. An
    /// extraction then writes `req: _`, and an import pass finds nothing to restore, with nothing in
    /// either answer to say why. The health is the one place the server says so.
    health: Option<(String, Option<String>)>,
    /// The furthest percentage any phase reported, and the phase it belonged to.
    ///
    /// Kept apart from `last` because the two answer different questions and the server routinely
    /// makes them disagree: it counts files inside a phase, then emits sub-steps carrying no
    /// percentage at all (`working: tddy_desktop (lib)`). A timeout landing on one of those had a
    /// `last` with no number in it, so the message could not say how far the index had got — which
    /// is the one thing a reader needs in order to decide whether raising the budget will help.
    furthest: Option<(u64, String)>,
}

impl ServerChatter {
    /// Fold one server-sent message in, and return the line worth printing for it.
    ///
    /// A message that answers a request carries no `method`, which is what keeps every result out
    /// of the progress stream without having to know the ids in flight.
    pub fn absorb(&mut self, message: &Value) -> Option<String> {
        match message.get("method").and_then(Value::as_str)? {
            "$/progress" => self.progress(message.get("params")?),
            "experimental/serverStatus" => {
                let params = message.get("params")?;
                if let Some(health) = params.get("health").and_then(Value::as_str) {
                    let said = params
                        .get("message")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    self.health = Some((health.to_string(), said));
                }
                self.quiescent = params.get("quiescent").and_then(Value::as_bool)?;
                self.reported_status = true;
                None
            }
            _ => None,
        }
    }

    /// Fold one `$/progress` notification in.
    ///
    /// The title arrives only with `begin`, so it is held and reused for the `report` lines that
    /// follow it — without that, a report reads as a bare percentage with nothing to attach it to.
    /// Lines are deduplicated on the phase and its percentage rather than on the whole line, because
    /// the server reports one notification per *file* scanned and each carries a different absolute
    /// path. Printing all of them buries the phases; one line per percent of each phase is the
    /// progress a reader can actually follow. A notification with no percentage — every `begin`, and
    /// the sub-steps of a phase that does not count — falls back to the line itself.
    fn progress(&mut self, params: &Value) -> Option<String> {
        let token = token_key(params.get("token")?);
        let value = params.get("value")?;
        match value.get("kind").and_then(Value::as_str)? {
            "begin" => {
                if let Some(title) = value.get("title").and_then(Value::as_str) {
                    self.titles.insert(token.clone(), title.to_string());
                }
            }
            "end" => {
                self.titles.remove(&token);
                return None;
            }
            _ => {}
        }

        let title = self.titles.get(&token).cloned();
        let line = progress_line(title.as_deref(), value);
        self.last = Some(line.clone());

        let percentage = value.get("percentage").and_then(Value::as_u64);
        if let Some(percentage) = percentage {
            self.reached(percentage, title.as_deref().unwrap_or(UNTITLED_PHASE));
        }

        let key = match percentage {
            Some(percentage) => percentage.to_string(),
            None => line.clone(),
        };
        if self.shown.get(&token) == Some(&key) {
            return None;
        }
        self.shown.insert(token, key);
        Some(line)
    }

    /// Keep `percentage` of `phase` as the furthest the index got, unless it has been further.
    fn reached(&mut self, percentage: u64, phase: &str) {
        if self
            .furthest
            .as_ref()
            .is_none_or(|(seen, _)| percentage >= *seen)
        {
            self.furthest = Some((percentage, phase.to_string()));
        }
    }
}

impl ServerChatter {
    /// Where the index got to, for a message that has to explain a timeout.
    ///
    /// The last line on its own is not enough: it is often a sub-step with no percentage. This
    /// pairs it with the furthest percentage seen, so the reader can tell a server that stalled at
    /// 12% from one that timed out at 99% — the first wants investigating, the second wants a
    /// bigger budget.
    pub fn how_far(&self) -> String {
        let last = self
            .last
            .clone()
            .unwrap_or_else(|| "nothing reported".to_string());
        match &self.furthest {
            Some((percentage, phase)) => format!("{last}; furthest {phase} {percentage}%"),
            None => last,
        }
    }

    /// Whether the server has reported its own graph loaded and queryable.
    ///
    /// `experimental/serverStatus` is an extension, so a `false` here means "has not said so",
    /// never "is not loaded" — which is why [`RustBackend::ensure_indexed`] treats it as a shortcut
    /// out of a hover probe rather than as the probe itself. A consumer with no probe available has
    /// only this, and must say so rather than presenting it as the stronger claim.
    pub fn quiescent(&self) -> bool {
        self.quiescent
    }

    /// Whether the server has said it is still loading, and has not yet said it has finished.
    ///
    /// Loading is more than the crate graph: rust-analyzer answers hover while it is still running
    /// build scripts and loading proc macros, and until then a type generated into `OUT_DIR` does
    /// not exist for it, and a name it will resolve is not yet reported as unresolved either. So a
    /// hover alone is not readiness while this is true. It is false for a server that has never
    /// sent the extension, which leaves hover the authority there.
    pub fn loading(&self) -> bool {
        self.reported_status && !self.quiescent
    }

    /// Why the index cannot be trusted, when rust-analyzer last reported its health as anything
    /// but `ok` — quoting the message it gave, which is the only account of the cause there is.
    ///
    /// **`warning` counts, not only `error`.** A failed build script comes through as `warning`
    /// ("Failed to run build scripts of some packages"), and that is exactly the state in which an
    /// extraction writes `_` for a type generated into `OUT_DIR` and the import pass has nothing to
    /// act on. rust-analyzer's other warnings — a manifest change it has not reloaded, build
    /// scripts or proc macros that changed and need rebuilding, a configuration it could not read,
    /// no workspace discovered — each also mean the graph it answers from is not the tree on disk.
    /// A run that went ahead would report success over answers nobody could vouch for, which is the
    /// implicit failure this exists to make explicit.
    ///
    /// `None` for a server that has not reported its health: silence is not a degraded index, and
    /// the extension is optional.
    pub fn degraded(&self) -> Option<String> {
        let (health, said) = self.health.as_ref()?;
        if health == "ok" {
            return None;
        }
        let quoted = match said
            .as_deref()
            .map(|message| message.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|message| !message.is_empty())
        {
            Some(message) => format!(": \"{message}\""),
            None => " and gives no reason.".to_string(),
        };
        Some(format!(
            "rust-analyzer reports its index as degraded (health `{health}`){quoted} An index in \
             that state answers without the code it could not load — a type a failed build script \
             should have generated does not exist for it — so no result it gives here can be \
             trusted. rust-analyzer runs build scripts in the environment it was started with, so \
             this can fail there while `cargo check` in the dev shell succeeds: a linker missing \
             the shell's flags is the usual cause. Start the server — or the index daemon, with \
             `./run-index-daemon --stop && ./run-index-daemon` — from the dev shell's whole \
             environment and run again."
        ))
    }

    /// The furthest percentage any phase has reported, and the phase it belonged to.
    ///
    /// Kept apart from the last line for the reason the field states: the server counts files
    /// inside a phase and then emits sub-steps carrying no percentage at all, so the last line is
    /// routinely the one with no number in it.
    pub fn furthest(&self) -> Option<(u64, &str)> {
        self.furthest
            .as_ref()
            .map(|(percentage, phase)| (*percentage, phase.as_str()))
    }
}

/// A progress token as a map key. The specification allows a string or a number.
fn token_key(token: &Value) -> String {
    match token.as_str() {
        Some(text) => text.to_string(),
        None => token.to_string(),
    }
}

/// One progress notification as a line: what the server is doing, where it has got to, and how far.
fn progress_line(title: Option<&str>, value: &Value) -> String {
    let mut line = title.unwrap_or(UNTITLED_PHASE).to_string();
    if let Some(message) = value.get("message").and_then(Value::as_str) {
        line.push_str(": ");
        line.push_str(message);
    }
    if let Some(percentage) = value.get("percentage").and_then(Value::as_u64) {
        line.push_str(&format!(" ({percentage}%)"));
    }
    line
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn a_status(quiescent: bool) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "experimental/serverStatus",
            "params": { "health": "ok", "quiescent": quiescent }
        })
    }

    /// A status carrying rust-analyzer's own health and the message it gives for it.
    fn a_status_reporting(health: &str, message: Option<&str>) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "experimental/serverStatus",
            "params": { "health": health, "quiescent": true, "message": message }
        })
    }

    const BUILD_SCRIPTS_FAILED: &str =
        "Failed to run build scripts of some packages.\n\nPlease refer to the logs for more details on the errors.";

    #[test]
    fn finds_nothing_degraded_in_a_server_that_reports_itself_healthy() {
        // Given
        let mut chatter = ServerChatter::default();

        // When
        chatter.absorb(&a_status_reporting("ok", None));

        // Then
        assert_eq!(chatter.degraded(), None);
    }

    #[test]
    fn finds_nothing_degraded_in_a_server_that_has_not_reported_its_health() {
        // Given
        let chatter = ServerChatter::default();

        // When
        let degraded = chatter.degraded();

        // Then
        assert_eq!(degraded, None);
    }

    #[test]
    fn reads_a_warning_as_a_degraded_index_quoting_what_the_server_said() {
        // Given
        let mut chatter = ServerChatter::default();

        // When
        chatter.absorb(&a_status_reporting("warning", Some(BUILD_SCRIPTS_FAILED)));

        // Then
        assert_eq!(
            chatter.degraded().as_deref(),
            Some(
                "rust-analyzer reports its index as degraded (health `warning`): \"Failed to run \
                 build scripts of some packages. Please refer to the logs for more details on the \
                 errors.\" An index in that state answers without the code it could not load — a \
                 type a failed build script should have generated does not exist for it — so no \
                 result it gives here can be trusted. rust-analyzer runs build scripts in the environment it was \
                 started with, so this can fail there while `cargo check` in the dev shell \
                 succeeds: a linker missing the shell's flags is the usual cause. Start the server \
                 — or the index daemon, with `./run-index-daemon --stop && ./run-index-daemon` — \
                 from the dev shell's whole environment and run again."
            )
        );
    }

    #[test]
    fn reads_an_error_as_a_degraded_index_even_when_the_server_gives_no_reason() {
        // Given
        let mut chatter = ServerChatter::default();

        // When
        chatter.absorb(&a_status_reporting("error", None));

        // Then
        assert_eq!(
            chatter.degraded().as_deref(),
            Some(
                "rust-analyzer reports its index as degraded (health `error`) and gives no reason. \
                 An index in that state answers without the code it could not load — a type a \
                 failed build script should have generated does not exist for it — so no result it \
                 gives here can be trusted. rust-analyzer runs build scripts in the environment it was \
                 started with, so this can fail there while `cargo check` in the dev shell \
                 succeeds: a linker missing the shell's flags is the usual cause. Start the server \
                 — or the index daemon, with `./run-index-daemon --stop && ./run-index-daemon` — \
                 from the dev shell's whole environment and run again."
            )
        );
    }

    #[test]
    fn keeps_only_the_latest_health_a_server_reported() {
        // Given
        let mut chatter = ServerChatter::default();
        chatter.absorb(&a_status_reporting("warning", Some(BUILD_SCRIPTS_FAILED)));

        // When
        chatter.absorb(&a_status_reporting("ok", None));

        // Then
        assert_eq!(chatter.degraded(), None);
    }

    #[test]
    fn reads_a_server_that_has_said_nothing_as_not_loading() {
        // Given
        let chatter = ServerChatter::default();

        // When
        let loading = chatter.loading();

        // Then
        assert!(
            !loading,
            "silence about status was read as a load in progress"
        );
    }

    #[test]
    fn reads_a_server_that_said_it_is_not_quiescent_as_loading() {
        // Given
        let mut chatter = ServerChatter::default();

        // When
        chatter.absorb(&a_status(false));

        // Then
        assert!(chatter.loading());
    }

    #[test]
    fn stops_reading_a_load_once_the_server_says_it_is_quiescent() {
        // Given
        let mut chatter = ServerChatter::default();
        chatter.absorb(&a_status(false));

        // When
        chatter.absorb(&a_status(true));

        // Then
        assert!(!chatter.loading());
    }
}

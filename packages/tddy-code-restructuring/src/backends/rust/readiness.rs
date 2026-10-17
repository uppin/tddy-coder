//! How the backend decides rust-analyzer is ready to be asked.
//!
//! Split out of `backends/rust.rs`, which is past its size budget.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::wait::server_name;
use super::{
    first_symbol_position, path_of, relative_to, seam_refusal, server_defect, LspPoint,
    RustBackend, WaitStage, Waiting, CANCEL_CHECK, INDEXING_POLL,
};
use crate::{RestructureError, Result};

/// The code rust-analyzer gives the diagnostic it attaches to code a `#[cfg]` has switched off.
const INACTIVE_CODE: &str = "inactive-code";

/// The code rust-analyzer gives the diagnostic it attaches to a file no crate's module tree reaches.
const UNLINKED_FILE: &str = "unlinked-file";

/// How long a ready index may leave a hover `null` at a position that has nothing excusing it.
///
/// A ready index answers a hover in well under a second. Past this the position is one the server
/// will never type, and waiting longer is how a range opening with `&` held a warm daemon for
/// minutes with the server idle.
pub const READY_HOVER_BOUND: Duration = Duration::from_secs(30);

/// How long a server that says it is loading may say nothing new before a wait calls it stalled.
///
/// A silence bound, not a deadline: every new word from the server restarts it, so a load that
/// keeps narrating — twenty minutes of indexing on this workspace — is never ended by it. Ten
/// minutes is past any single build script this workspace runs, and is where the `#carve` 17
/// stage B2 apply had already been killed by hand. Reverses `#sharpen` 4/8's "no deadline" for
/// silence only, with the developer's consent (`#reshape` 9/19).
pub const LOADING_SILENCE_BOUND: Duration = Duration::from_secs(600);

/// The two silences a wait for type inference tolerates before it refuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SilenceBounds {
    /// A ready index leaving the hover `null` at a position nothing excuses.
    pub ready: Duration,
    /// A loading server whose last words have not changed.
    pub loading: Duration,
}

impl Default for SilenceBounds {
    fn default() -> Self {
        SilenceBounds {
            ready: READY_HOVER_BOUND,
            loading: LOADING_SILENCE_BOUND,
        }
    }
}

/// What one look at a silent wait decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Silence {
    /// Keep waiting: nothing has been silent past its bound.
    Waiting,
    /// The index is ready and has left the hover `null` past [`SilenceBounds::ready`], for this
    /// long.
    ReadyUntypable(Duration),
    /// The server says it is loading and its words have not changed past
    /// [`SilenceBounds::loading`].
    Stalled,
}

/// What a wait that has seen no answer decides, from the server's state and its two silences.
///
/// `ready_silent_for` is how long a ready index has left the hover `null` (`None` while it is
/// loading); `words_unchanged_for` is how long the server's last words have stood. Neither is how
/// long the wait has lasted: a server that keeps saying something new is never refused here.
pub(super) fn silence_verdict(
    loading: bool,
    ready_silent_for: Option<Duration>,
    words_unchanged_for: Duration,
    bounds: &SilenceBounds,
) -> Silence {
    if loading {
        if silent_past(words_unchanged_for, bounds.loading) {
            Silence::Stalled
        } else {
            Silence::Waiting
        }
    } else {
        match ready_silent_for {
            Some(silent_for) if silent_past(silent_for, bounds.ready) => {
                Silence::ReadyUntypable(silent_for)
            }
            _ => Silence::Waiting,
        }
    }
}

/// Whether a silence of `silent_for` has outlasted `bound`.
fn silent_past(silent_for: Duration, bound: Duration) -> bool {
    silent_for >= bound
}

/// Where a wait for one position ended.
pub(super) enum Answerable {
    /// The server resolves names here.
    Ready,
    /// The server reports the code here as inactive, quoting its own reason. It resolves no name in
    /// such code however long it is given, so this ends a wait as surely as [`Answerable::Ready`].
    Inactive(String),
}

impl RustBackend {
    /// Wait, once per process, for the crate graph to load — with the server's progress on screen.
    ///
    /// Every request that needs name resolution is answered emptily until rust-analyzer has loaded
    /// the graph, so waiting for it here once is what keeps every later wait short. On a real crate
    /// the alternative is paid per operation, and `survey_moved_items` pays it per moved item.
    ///
    /// Hover is the authority, because it is the cheapest request that needs the graph and it is the
    /// same signal a rename is gated on. `serverStatus` is a shortcut out when it says quiescent,
    /// and a reason to keep waiting while it says loading: hover answers before build scripts and
    /// proc macros have loaded, and an extraction taken then writes `req: _` for a type generated
    /// into `OUT_DIR`, while the import pass finds no unresolved name to restore. It is an
    /// extension, so a server that never sends it still gets past this on hover alone.
    ///
    /// A document with no symbols has nothing to hover, so the warm-up is skipped rather than spent
    /// on a position that would never resolve — which leaves `indexed` false, and the first real
    /// wait doing the waiting instead.
    ///
    /// Bounded on silence as the wait for type inference is: a server that says it is loading and
    /// then says nothing new for [`SilenceBounds::loading`] is refused as
    /// [`RestructureError::ServerStalled`] in this stage. There is no bound on total time.
    pub(super) fn ensure_indexed(&mut self, uri: &str) -> Result<()> {
        if self.indexed {
            return self.refuse_degraded_index();
        }
        (self.progress)("warming crate index (until ready, or until you stop waiting)");
        let symbols = self.request_settled(
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": uri } }),
        )?;
        let Some(probe) = first_symbol_position(&symbols) else {
            (self.progress)("no indexable symbols in file; skipping warm-up");
            return Ok(());
        };

        let started = Instant::now();
        let mut waiting = Waiting::begin(
            WaitStage::warming_the_crate_index(),
            self.wait_heartbeat,
            started,
        );
        loop {
            let hover = self.request_settled(
                "textDocument/hover",
                json!({ "textDocument": { "uri": uri }, "position": probe }),
            )?;

            if (!hover.is_null() || self.chatter.quiescent()) && !self.chatter.loading() {
                self.indexed = true;
                (self.progress)("crate index ready");
                return self.refuse_degraded_index();
            }
            if !self.keep_waiting_unless_stalled(&mut waiting, uri, &probe)? {
                return Err(self.incomplete_index(waiting.stage(), started.elapsed()));
            }
        }
    }

    /// Block until the server can resolve names at `position`.
    ///
    /// `documentSymbol` is answered from the syntax tree and so succeeds immediately, but a rename
    /// needs the crate graph. Hover is the cheapest request that also needs it, so a non-null hover
    /// is the signal that a rename will be accepted — once the server is not still loading, for the
    /// reason [`Self::ensure_indexed`] gives.
    ///
    /// Code rust-analyzer treats as inactive is refused rather than waited on: nothing resolves
    /// there, so an operation acting at it has nothing to act on.
    ///
    /// Bounded on silence, as every wait for type inference is: see [`Self::wait_until_answerable`].
    pub(super) fn wait_until_resolved(&mut self, uri: &str, position: &Value) -> Result<()> {
        match self.wait_until_answerable(uri, position)? {
            Answerable::Ready => Ok(()),
            Answerable::Inactive(said) => Err(inactive_code_refusal(uri, position, &said)?),
        }
    }

    /// Block until the server either resolves names at `position` or says it never will.
    ///
    /// A hover is `null` in three cases, and only one of them passes. Until the crate graph is
    /// loaded, nothing resolves anywhere. Once it is, a `null` can also mean the position is in code
    /// a `#[cfg]` has switched off, which rust-analyzer lists in the outline but never resolves. The
    /// wait once treated both cases as the first and polled until its caller gave up: lifecycle plan
    /// 02a's `check --deep` never finished, on `pty_runtime.rs`'s
    /// `#[cfg(not(unix))] fn resolve_final_argv_env` on macOS. Or the file is in no crate's module
    /// tree as the server has loaded it — no `mod` declares it, or the server was never told of the
    /// one that does — which is how a warm `check --deep` of a module an earlier apply had created
    /// never answered. The server says which case it is through its pull diagnostics, and that
    /// answer is asked for only once the index is loaded. Before that, a missing diagnostic proves
    /// nothing. A file the server reports `unlinked-file` is refused, since nothing resolves there
    /// however long the wait; code it reports inactive ends the wait for the caller to decide.
    ///
    /// Two silences end it too ([`SilenceBounds`]): a ready index leaving the hover `null` with
    /// nothing excusing it is a position it will never type, and a loading server saying nothing new
    /// is stalled ([`RestructureError::ServerStalled`]). Neither is a limit on total time.
    pub(super) fn wait_until_answerable(
        &mut self,
        uri: &str,
        position: &Value,
    ) -> Result<Answerable> {
        (self.progress)("waiting for type inference at the anchor");
        let started = Instant::now();
        let file = relative_to(uri, &self.workspace_root()?)?;
        let line = LspPoint::read(Some(position))?.line as u32 + 1;
        let mut waiting = Waiting::begin(
            WaitStage::type_inference_at(&file, line),
            self.wait_heartbeat,
            started,
        );
        let mut ready_and_silent_since: Option<Instant> = None;
        loop {
            let hover = self.request_settled(
                "textDocument/hover",
                json!({ "textDocument": { "uri": uri }, "position": position }),
            )?;

            if !hover.is_null() && !self.chatter.loading() {
                self.indexed = true;
                self.refuse_degraded_index()?;
                return Ok(Answerable::Ready);
            }
            let ready_and_silent = hover.is_null() && self.indexed && !self.chatter.loading();
            if ready_and_silent {
                if let Some(excused) = self.excused_silence(uri, position)? {
                    return Ok(excused);
                }
            } else {
                ready_and_silent_since = None;
            }
            let silent_for = ready_and_silent.then(|| {
                ready_and_silent_since
                    .get_or_insert_with(Instant::now)
                    .elapsed()
            });
            self.refuse_silence(&waiting, silent_for, uri, position)?;
            if !self.keep_waiting_unless_stalled(&mut waiting, uri, position)? {
                return Err(self.incomplete_index(waiting.stage(), started.elapsed()));
            }
        }
    }

    /// What a ready index's `null` hover at `position` means, when its diagnostics say: refused for
    /// a file no crate's module tree reaches, [`Answerable::Inactive`] for code a `#[cfg]` switched
    /// off, and `None` when nothing excuses it.
    fn excused_silence(&mut self, uri: &str, position: &Value) -> Result<Option<Answerable>> {
        let report = self.pull_diagnostics(uri)?;
        if let Some(said) = unlinked_file_in(&report) {
            self.refuse_degraded_index()?;
            return Err(unlinked_file_refusal(uri, &said)?);
        }
        if let Some(said) = inactive_code_covering(&report, &LspPoint::read(Some(position))?) {
            self.refuse_degraded_index()?;
            return Ok(Some(Answerable::Inactive(said)));
        }
        Ok(None)
    }

    /// Refuse a wait whose server has gone silent past its bound ([`silence_verdict`]): a ready
    /// index that has left the hover at `position` `null` for `silent_for`, with nothing excusing
    /// it, is a position it will never type; a loading server whose words have not changed is
    /// stalled.
    fn refuse_silence(
        &self,
        waiting: &Waiting,
        silent_for: Option<Duration>,
        uri: &str,
        position: &Value,
    ) -> Result<()> {
        let unchanged = self.chatter.quiet_for(Instant::now());
        let loading = self.chatter.loading();
        match silence_verdict(loading, silent_for, unchanged, &self.silence_bounds) {
            Silence::Waiting => Ok(()),
            Silence::Stalled => Err(self.stalled(waiting.stage(), unchanged)),
            Silence::ReadyUntypable(silent_for) => Err(server_defect(format!(
                "the index is ready and rust-analyzer still gives no hover at {} after \
                 {silent_for:?}, so it cannot type this position. Start the range on an expression \
                 it can type.",
                located(uri, position)?,
            ))),
        }
    }

    /// Wait out one poll of a wait on the server, as [`Self::keep_waiting`] does — `false` once the
    /// caller has stopped waiting — refusing a loading server gone silent ([`Self::refuse_silence`]).
    ///
    /// Both waits poll through this: the crate-index warm-up and the wait for type inference at
    /// `position`. A loading server that says nothing new is as stalled in the one as in the other.
    /// The ready bound is not judged here — a slice has no hover to judge it on.
    ///
    /// Judged in every slice the poll sleeps in rather than once per poll: a server's busy report
    /// can be folded away by the next poll's request before a once-per-poll look sees it.
    fn keep_waiting_unless_stalled(
        &self,
        waiting: &mut Waiting,
        uri: &str,
        position: &Value,
    ) -> Result<bool> {
        let until = Instant::now() + INDEXING_POLL;
        loop {
            self.refuse_silence(waiting, None, uri, position)?;
            let remaining = until.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(true);
            }
            if !self.keep_waiting(Some(waiting), remaining.min(CANCEL_CHECK)) {
                return Ok(false);
            }
        }
    }

    /// The refusal of a wait in `stage` on a loading server that has said nothing new for `quiet`.
    fn stalled(&self, stage: &WaitStage, quiet: Duration) -> RestructureError {
        let nothing = || "nothing reported".to_string();
        RestructureError::ServerStalled {
            stage: stage.text().to_string(),
            server: server_name(self.waited_on()),
            last: self.chatter.last.clone().unwrap_or_else(nothing),
            quiet_seconds: quiet.as_secs(),
            furthest: self
                .chatter
                .furthest()
                .map(|(percentage, phase)| format!("{phase} {percentage}%"))
                .unwrap_or_else(nothing),
        }
    }

    /// The server's pull-diagnostics report for `uri`.
    fn pull_diagnostics(&mut self, uri: &str) -> Result<Value> {
        self.request_settled(
            "textDocument/diagnostic",
            json!({ "textDocument": { "uri": uri } }),
        )
    }

    /// Refuse to go on once ready, when rust-analyzer has said its index is degraded.
    ///
    /// Ready is not trustworthy: a server whose build scripts failed still finishes loading, and
    /// then answers as though what they generate did not exist — which is how an extraction came
    /// to write `req: _` against an index everyone called warm. Checked at every point readiness is
    /// declared, so it holds for the first wait of a run and for each later one, on a server this
    /// backend started and on a warm one it was handed: the bridge folds in the server's latest
    /// status even when another consumer drained the transition. The reasoning for refusing on
    /// `warning` as well as `error` is at [`super::ServerChatter::degraded`].
    pub(super) fn refuse_degraded_index(&self) -> Result<()> {
        match self.chatter.degraded() {
            Some(reason) => Err(server_defect(reason)),
            None => Ok(()),
        }
    }
}

/// The message of the `inactive-code` diagnostic covering `point`, in a pull-diagnostics report.
fn inactive_code_covering(report: &Value, point: &LspPoint) -> Option<String> {
    let at = (point.line, point.character);
    report
        .get("items")?
        .as_array()?
        .iter()
        .filter(|item| item.get("code").and_then(Value::as_str) == Some(INACTIVE_CODE))
        .find(|item| {
            let bound = |pointer: &str| {
                LspPoint::read(item.pointer(pointer))
                    .ok()
                    .map(|bound| (bound.line, bound.character))
            };
            match (bound("/range/start"), bound("/range/end")) {
                (Some(start), Some(end)) => (start..=end).contains(&at),
                _ => false,
            }
        })
        .map(|item| {
            item.get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        })
}

/// The first paragraph of the `unlinked-file` diagnostic in a pull-diagnostics report, if any.
///
/// The diagnostic covers the whole file, so its range says nothing about any one position. Its
/// message goes on to say how to silence it in an editor, which is no advice for this caller.
fn unlinked_file_in(report: &Value) -> Option<String> {
    report
        .get("items")?
        .as_array()?
        .iter()
        .find(|item| item.get("code").and_then(Value::as_str) == Some(UNLINKED_FILE))
        .map(|item| {
            let message = item
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default();
            message.split("\n\n").next().unwrap_or(message).to_string()
        })
}

fn unlinked_file_refusal(uri: &str, said: &str) -> Result<RestructureError> {
    Ok(seam_refusal(format!(
        "{} is in no crate's module tree as rust-analyzer has loaded it (\"{said}\"), so nothing \
         resolves there however long the wait. Declare it with `mod` in its parent module, or \
         anchor the operation in a file that is declared.",
        path_of(uri)?.display()
    )))
}

/// Where a position is, as `path:line`, for a message a reader has to find it from.
pub(super) fn located(uri: &str, position: &Value) -> Result<String> {
    let point = LspPoint::read(Some(position))?;
    Ok(format!("{}:{}", path_of(uri)?.display(), point.line + 1))
}

fn inactive_code_refusal(uri: &str, position: &Value, said: &str) -> Result<RestructureError> {
    Ok(seam_refusal(format!(
        "{} is code rust-analyzer treats as inactive (\"{said}\"), so it resolves nothing there. \
         Run against a server configured with the cfg that activates it, or leave that code out \
         of the operation.",
        located(uri, position)?
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bounds a test reads easily: a ready bound of 30 s and a loading bound of 10 min.
    fn the_default_bounds() -> SilenceBounds {
        SilenceBounds::default()
    }

    /// `#reshape` 9/19: a server that says it is loading and has said nothing new for longer than
    /// the loading bound is stalled, not slow.
    #[test]
    fn a_loading_server_whose_words_are_unchanged_past_the_loading_bound_is_stalled() {
        // Given a loading server silent for one second past the loading bound
        let silent_for = LOADING_SILENCE_BOUND + Duration::from_secs(1);

        // When the wait looks at it
        let verdict = silence_verdict(true, None, silent_for, &the_default_bounds());

        // Then it is stalled
        assert_eq!(verdict, Silence::Stalled);
    }

    /// The bound measures silence, not time: a loading server that said something new a moment ago
    /// is waited on however long the wait has lasted.
    #[test]
    fn a_loading_server_with_new_words_inside_the_bound_is_waited_on() {
        // Given a loading server whose last words are one second old
        let silent_for = Duration::from_secs(1);

        // When the wait looks at it
        let verdict = silence_verdict(true, None, silent_for, &the_default_bounds());

        // Then the wait goes on
        assert_eq!(verdict, Silence::Waiting);
    }

    /// rust-analyzer's own answer for `#[cfg(not(rust_analyzer))] fn …` spanning lines 9–12.
    fn a_report_of_inactive_code_on_lines_9_to_12() -> Value {
        json!({
            "kind": "full",
            "items": [{
                "range": {
                    "start": { "line": 9, "character": 0 },
                    "end": { "line": 12, "character": 1 }
                },
                "severity": 4,
                "code": "inactive-code",
                "message": "code is inactive due to #[cfg] directives: rust_analyzer is enabled"
            }]
        })
    }

    #[test]
    fn reads_the_server_s_reason_at_a_position_inside_inactive_code() {
        // Given
        let report = a_report_of_inactive_code_on_lines_9_to_12();

        // When
        let said = inactive_code_covering(
            &report,
            &LspPoint {
                line: 10,
                character: 3,
            },
        );

        // Then
        assert_eq!(
            said.as_deref(),
            Some("code is inactive due to #[cfg] directives: rust_analyzer is enabled")
        );
    }

    #[test]
    fn finds_nothing_at_a_position_outside_inactive_code() {
        // Given
        let report = a_report_of_inactive_code_on_lines_9_to_12();

        // When
        let said = inactive_code_covering(
            &report,
            &LspPoint {
                line: 5,
                character: 3,
            },
        );

        // Then
        assert_eq!(said, None);
    }

    /// rust-analyzer's own answer for a file no `mod` declares, as a live server gave it.
    fn a_report_of_an_unlinked_file() -> Value {
        json!({
            "kind": "full",
            "resultId": "rust-analyzer",
            "items": [{
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 3, "character": 0 }
                },
                "severity": 4,
                "code": "unlinked-file",
                "source": "rust-analyzer",
                "message": "This file is not included anywhere in the module tree, so \
                            rust-analyzer can't offer IDE services.\n\nIf you're intentionally \
                            working on unowned files, you can silence this warning by adding \
                            \"unlinked-file\" to rust-analyzer.diagnostics.disabled in your \
                            settings."
            }]
        })
    }

    #[test]
    fn reads_the_server_s_reason_for_a_file_no_module_declares() {
        // Given
        let report = a_report_of_an_unlinked_file();

        // When
        let said = unlinked_file_in(&report);

        // Then
        assert_eq!(
            said.as_deref(),
            Some(
                "This file is not included anywhere in the module tree, so rust-analyzer can't \
                 offer IDE services."
            )
        );
    }

    #[test]
    fn finds_no_unlinked_file_in_a_report_of_inactive_code() {
        // Given
        let report = a_report_of_inactive_code_on_lines_9_to_12();

        // When
        let said = unlinked_file_in(&report);

        // Then
        assert_eq!(said, None);
    }

    #[test]
    fn ignores_a_diagnostic_that_is_not_about_inactive_code() {
        // Given a warning covering the position, of another kind
        let report = json!({
            "kind": "full",
            "items": [{
                "range": {
                    "start": { "line": 9, "character": 0 },
                    "end": { "line": 12, "character": 1 }
                },
                "code": "unused_variables",
                "message": "unused variable"
            }]
        });

        // When
        let said = inactive_code_covering(
            &report,
            &LspPoint {
                line: 10,
                character: 3,
            },
        );

        // Then
        assert_eq!(said, None);
    }

    #[test]
    fn a_hover_silent_for_exactly_the_bound_has_outlasted_it() {
        // Given a ready index that has left the hover null for the whole bound
        let silent_for = READY_HOVER_BOUND;

        // When
        let outlasted = silent_past(silent_for, READY_HOVER_BOUND);

        // Then
        assert!(outlasted);
    }

    #[test]
    fn a_hover_silent_for_less_than_the_bound_has_not_outlasted_it() {
        // Given
        let silent_for = READY_HOVER_BOUND - Duration::from_millis(1);

        // When
        let outlasted = silent_past(silent_for, READY_HOVER_BOUND);

        // Then
        assert!(!outlasted);
    }
}

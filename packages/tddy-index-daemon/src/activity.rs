//! What this process says about its own activity: which request arrived, for which tree, whether
//! that tree was already warm, how it ended and how long it took.
//!
//! Without these lines a served `Check` left the log holding nothing but `listening on …`, and an
//! operator could not tell a working daemon from a wedged one, could not see which roots were warm,
//! and had no record of a client that disconnected mid-run. That is this process's own state, and
//! it is the one thing no client can observe for it.
//!
//! **The lines are composed by pure functions and logged by [`Activity`].** Composing and writing
//! are split because the wording is what a test can hold: `log` output is awkward to capture, and a
//! harness that captured it would pin the logger rather than the sentence. The sentences are
//! therefore `assert_eq!`-able strings, and `Activity` is the thin part that measures a clock and
//! calls `log::info!`.
//!
//! One target — `tddy_index_daemon::activity` — for every line about a request, whichever module
//! serves it. The same reasoning the binary's `MAIN` target states: a log policy can turn request
//! activity up or down without touching what a module says about its internals, and an operator
//! reading `RUST_LOG=tddy_index_daemon::activity=info` gets exactly the request journal.
//!
//! **INFO for arrival and outcome, and nothing per-operation.** A 55-minute coverage capture
//! reports per test, and a line per test at INFO would bury the two lines that matter; those stay
//! on their modules' own DEBUG targets.

use std::path::{Path, PathBuf};
use std::time::Instant;

use tddy_code_restructuring::restructure_cli::step_delta;
use tddy_rpc::Status;

use crate::index::WorkspaceIndex;

/// The target every line about a request carries.
const ACTIVITY: &str = "tddy_index_daemon::activity";

/// Whether the tree a request named already had an index when the request arrived.
///
/// The single most useful fact the daemon can report, because it is the difference the whole crate
/// exists for: a warm root answers in milliseconds, a cold one pays a six-to-ten-minute
/// crate-graph load first. A wait nobody explained reads as a hang.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Warmth {
    /// This process already holds a language server for that root.
    AlreadyWarm,
    /// Nothing has loaded that root yet, so whatever this request waits for, it waits for that.
    NotYetIndexed,
}

/// How a request ended.
pub(crate) enum Ending<'a> {
    /// The answer was produced and sent.
    Answered,
    /// The request was refused, with the status its caller received.
    Refused(&'a Status),
    /// The caller went away before the work finished, so the work was stopped. Not a refusal:
    /// nothing was wrong with the request.
    Cancelled,
}

/// One request, from its arrival to whatever it came to.
///
/// Holds the root it named and the moment it arrived, so the outcome line can name both without
/// the handler having to carry them along beside it.
pub(crate) struct Activity {
    method: &'static str,
    root: PathBuf,
    arrived: Instant,
}

impl Activity {
    /// Record `method` arriving for the root `requested` names, and resolve that root.
    ///
    /// The resolution lives here rather than beside the call because a request this process cannot
    /// even attribute to a tree is still a request that arrived: an operator who saw nothing at all
    /// would read a client naming a relative root as a silent client.
    pub(crate) async fn arrived(
        method: &'static str,
        index: &WorkspaceIndex,
        requested: &str,
    ) -> Result<(Self, PathBuf), Status> {
        let arrived = Instant::now();
        let root = match WorkspaceIndex::workspace_root_of(requested) {
            Ok(root) => root,
            Err(refusal) => {
                log::info!(
                    target: ACTIVITY,
                    "{}",
                    outcome_line(
                        method,
                        Path::new(requested),
                        &Ending::Refused(&refusal),
                        &step_delta(Some(arrived), Instant::now()),
                    )
                );
                return Err(refusal);
            }
        };
        log::info!(
            target: ACTIVITY,
            "{}",
            arrival_line(method, &root, index.warmth_of(&root).await)
        );
        Ok((
            Self {
                method,
                root: root.clone(),
                arrived,
            },
            root,
        ))
    }

    /// The name this request is logged under, which is also the name its failures are attributed
    /// to.
    pub(crate) fn method(&self) -> &'static str {
        self.method
    }

    pub(crate) fn answered(&self) {
        self.ended(&Ending::Answered);
    }

    pub(crate) fn refused(&self, status: &Status) {
        self.ended(&Ending::Refused(status));
    }

    pub(crate) fn cancelled(&self) {
        self.ended(&Ending::Cancelled);
    }

    /// Log how `outcome` ended this request, and hand it back untouched.
    ///
    /// For the unary handlers, whose whole answer *is* a `Result`: recording the outcome next to
    /// the `?` that produces it is what keeps a refusal from leaving the process unremarked.
    ///
    /// **Consumes the activity**, so one request cannot be recorded as ending twice. That is not
    /// pedantry: the first version of this logging used it on an intermediate path resolution as
    /// well as on the run, and every answered check was reported as answered twice. Taking `self`
    /// turns that into a compile error. An intermediate step that can only *fail* the request uses
    /// [`Self::refusing`] instead.
    pub(crate) fn recorded<T>(self, outcome: Result<T, Status>) -> Result<T, Status> {
        match &outcome {
            Ok(_) => self.answered(),
            Err(refusal) => self.refused(refusal),
        }
        outcome
    }

    /// Log `outcome` if it ends this request, and hand it back untouched.
    ///
    /// For a resolution a streaming handler does before it spawns: succeeding is not the request
    /// being answered — the answer comes later, from the task — so only the refusal is recorded.
    pub(crate) fn refusing<T>(&self, outcome: Result<T, Status>) -> Result<T, Status> {
        if let Err(refusal) = &outcome {
            self.refused(refusal);
        }
        outcome
    }

    fn ended(&self, ending: &Ending<'_>) {
        log::info!(
            target: ACTIVITY,
            "{}",
            outcome_line(
                self.method,
                &self.root,
                ending,
                &step_delta(Some(self.arrived), Instant::now()),
            )
        );
    }
}

/// The line that says a request arrived, for which tree, and what this process already holds for
/// it.
pub(crate) fn arrival_line(method: &str, root: &Path, warmth: Warmth) -> String {
    format!(
        "{method} arrived for `{}`, which {}",
        root.display(),
        match warmth {
            Warmth::AlreadyWarm => "is already warm",
            Warmth::NotYetIndexed => "has no index yet",
        }
    )
}

/// The line that says how a request ended and how long it took to get there.
///
/// `stamp` is passed in rather than measured here — as
/// [`tddy_code_restructuring::restructure_cli::step_delta`] renders it, the same vocabulary the
/// command line's narration uses — so this stays a pure function whose whole sentence a test can
/// hold.
pub(crate) fn outcome_line(method: &str, root: &Path, ending: &Ending<'_>, stamp: &str) -> String {
    let ended = match ending {
        Ending::Answered => format!("answered ({stamp})"),
        // The code travels with the message because it is what an operator acts differently on: a
        // `FailedPrecondition` is a tree to fix, an `InvalidArgument` is a client to fix.
        Ending::Refused(status) => format!(
            "refused as {:?} ({stamp}): {}",
            status.code(),
            status.message()
        ),
        Ending::Cancelled => format!("cancelled ({stamp}): its caller stopped listening"),
    };
    format!("{method} for `{}`: {ended}", root.display())
}

/// The line that says a root has stopped being warm.
///
/// Worth its own line because it is the only thing that explains the next request for that root
/// paying the crate-graph load again — the registry reaps on its own schedule, so nothing a client
/// does accounts for it.
pub(crate) fn reaped_line(root: &Path) -> String {
    format!(
        "`{}` is no longer warm: its language server has been reaped",
        root.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tddy_rpc::Status;

    /// The line an operator reads to tell a working daemon from a wedged one: which method arrived,
    /// and for which tree.
    #[test]
    fn an_arriving_request_names_the_method_and_the_tree_it_is_for() {
        // Given a check arriving for a root this process already holds an index for
        // When the arrival is composed
        let line = arrival_line("check", Path::new("/trees/one"), Warmth::AlreadyWarm);

        // Then it names both, and says the request will not pay a crate-graph load
        assert_eq!(
            line,
            "check arrived for `/trees/one`, which is already warm"
        );
    }

    /// The single most useful line the daemon can emit: warm or not is the difference the whole
    /// crate exists for — milliseconds against six to ten minutes.
    #[test]
    fn an_arriving_request_says_when_the_tree_it_names_has_no_index_yet() {
        // Given a check arriving for a root nothing has loaded
        // When the arrival is composed
        let line = arrival_line("check", Path::new("/trees/two"), Warmth::NotYetIndexed);

        // Then it says so, rather than leaving the wait unexplained
        assert_eq!(
            line,
            "check arrived for `/trees/two`, which has no index yet"
        );
    }

    #[test]
    fn an_answered_request_is_reported_with_how_long_it_took() {
        // Given a check that was answered
        // When its outcome is composed
        let line = outcome_line("check", Path::new("/trees/one"), &Ending::Answered, "+1.2s");

        // Then the answer and the time it took are one line
        assert_eq!(line, "check for `/trees/one`: answered (+1.2s)");
    }

    /// The code travels with the message because it is what an operator acts differently on: a
    /// `FailedPrecondition` tree is a tree to fix, an `InvalidArgument` is a client to fix.
    #[test]
    fn a_refused_request_carries_the_status_code_its_caller_was_given() {
        // Given a check refused for naming no plan
        let refusal = Status::invalid_argument("the request names no plan");

        // When its outcome is composed
        let line = outcome_line(
            "check",
            Path::new("/trees/one"),
            &Ending::Refused(&refusal),
            "+30ms",
        );

        // Then the code and the reason both reach the log
        assert_eq!(
            line,
            "check for `/trees/one`: refused as InvalidArgument (+30ms): the request names no plan"
        );
    }

    /// A client that disconnected mid-run is the one outcome the daemon otherwise leaves no record
    /// of at all, and it is not a refusal: nothing was wrong with the request.
    #[test]
    fn a_request_whose_caller_went_away_is_reported_as_cancelled() {
        // Given an apply nobody is listening to any more
        // When its outcome is composed
        let line = outcome_line(
            "apply",
            Path::new("/trees/one"),
            &Ending::Cancelled,
            "+2.0s",
        );

        // Then the record says the caller left rather than that the run failed
        assert_eq!(
            line,
            "apply for `/trees/one`: cancelled (+2.0s): its caller stopped listening"
        );
    }

    /// A root whose server has been reaped is no longer warm, and the next request for it pays the
    /// load again — which is unreadable unless the reaping itself is on the record.
    #[test]
    fn a_reaped_root_is_reported_as_no_longer_warm() {
        // Given a root whose language server the registry has taken away
        // When the reaping is composed
        let line = reaped_line(Path::new("/trees/one"));

        // Then the log says that root has stopped being warm, and why
        assert_eq!(
            line,
            "`/trees/one` is no longer warm: its language server has been reaped"
        );
    }
}

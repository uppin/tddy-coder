//! What a run says while it waits for rust-analyzer.
//!
//! A wait that lasts minutes and has no budget — a run waits until the server is ready or until
//! its caller stops it — is silent once the server stops talking: the throttled progress stream
//! shows a phase once and then nothing while the server sits busy. This module is the heartbeat
//! that closes that: a [`Waiting`] gives a wait a stage and a clock, and [`heartbeat_line`] turns
//! what is known into one line for the run's own `progress` sink.
//!
//! **It adds no deadline.** Nothing here ends a wait, and nothing here sets a readiness flag: the
//! heartbeat reads the chatter and the clock and writes neither.
//!
//! **It prints nothing.** The line goes back to the caller as a string; the only printer in this
//! crate stays `restructure_cli.rs`.

// TODO(apply-heartbeat): the five polling waits adopt `Waiting` and nothing calls it yet.
#![allow(dead_code)]

use std::time::{Duration, Instant};

/// How often a wait that lasts says what it is waiting for.
///
/// Below the minute at which a developer starts to suspect a hang, above the two seconds at which a
/// healthy load already narrates itself. Not a budget: nothing ends at this number.
pub const WAIT_HEARTBEAT: Duration = Duration::from_secs(30);

/// What a wait names as the thing it is waiting for, until the waits name their own.
///
/// TODO(apply-heartbeat): replaced, wait by wait, by the [`WaitStage`] each one is in.
pub(super) const A_WAIT_THAT_NAMES_NO_STAGE: &str = "waiting for the index";

/// The part of a run a wait is in, as the sentence a reader is told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WaitStage(String);

impl WaitStage {
    pub(super) fn warming_the_crate_index() -> Self {
        WaitStage("warming the crate index".to_string())
    }

    pub(super) fn type_inference_at(file: &str, line: u32) -> Self {
        WaitStage(format!("type inference at {file}:{line}"))
    }

    pub(super) fn an_answer_for_the_assist(assist: &str, file: &str, line: u32) -> Self {
        WaitStage(format!(
            "an answer for the {assist} assist at {file}:{line}"
        ))
    }

    pub(super) fn the_outline_of(file: &str) -> Self {
        WaitStage(format!("the outline of {file}"))
    }

    pub(super) fn locating(name: &str, file: &str) -> Self {
        WaitStage(format!("locating {name} in {file}"))
    }

    pub(super) fn a_request_in_flight(method: &str) -> Self {
        WaitStage(format!("a {method} request in flight"))
    }

    pub(super) fn text(&self) -> &str {
        &self.0
    }
}

/// The server a wait is waiting on, as far as this repository can name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WaitedOn {
    /// A rust-analyzer this backend started, whose process it owns.
    OwnServer { pid: u32 },
    /// A client shared with other runs: the process is the host's, and not known here.
    SharedClient,
}

/// Everything one heartbeat line says.
pub(super) struct Beat<'a> {
    pub(super) stage: &'a WaitStage,
    /// How long *this wait* has lasted.
    pub(super) elapsed: Duration,
    pub(super) on: WaitedOn,
    /// Whether the server has said it is still loading.
    pub(super) loading: bool,
    /// The server's last words, or `None` when it has said nothing at all.
    pub(super) last_words: Option<&'a str>,
    /// How long ago the server last said anything new.
    pub(super) quiet_for: Duration,
    /// The furthest percentage a phase reported, and the phase.
    pub(super) furthest: Option<(u64, &'a str)>,
}

/// One wait's clock: when it began and when it last spoke.
pub(super) struct Waiting {
    stage: WaitStage,
    every: Duration,
    began: Instant,
    last_beat: Instant,
}

impl Waiting {
    pub(super) fn begin(stage: WaitStage, every: Duration, now: Instant) -> Self {
        Waiting {
            stage,
            every,
            began: now,
            last_beat: now,
        }
    }

    pub(super) fn stage(&self) -> &WaitStage {
        &self.stage
    }

    /// How long this wait has lasted at `now`.
    pub(super) fn elapsed(&self, now: Instant) -> Duration {
        now.duration_since(self.began)
    }

    /// Whether a beat is due at `now`, which is when one is owed and the clock then restarts.
    ///
    /// A fixed heartbeat: a server that is active does not postpone it, and one that is quiet does
    /// not bring it forward.
    pub(super) fn beat_due(&mut self, now: Instant) -> bool {
        // TODO(apply-heartbeat): implement. Never due until then, which is how every wait behaved
        // before there was a heartbeat.
        let _ = (now, self.every, self.last_beat);
        false
    }
}

/// The line a beat says, for the run's `progress` sink.
///
/// Fields, in order: the greppable prefix `still waiting`, how long this wait has lasted, the
/// stage, which server, whether it has said it is loading, its last words with how long they have
/// been unchanged, and the furthest phase it reported.
pub(super) fn heartbeat_line(beat: &Beat<'_>) -> String {
    // TODO(apply-heartbeat): implement. Empty until then: a line nobody sends.
    let _ = beat;
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    fn a_beat_in(stage: &WaitStage) -> Beat<'_> {
        Beat {
            stage,
            elapsed: Duration::from_secs(120),
            on: WaitedOn::SharedClient,
            loading: true,
            last_words: Some("working: build script num-bigint run"),
            quiet_for: Duration::from_secs(118),
            furthest: Some((100, "Roots Scanned")),
        }
    }

    fn the_line_of(beat: Beat<'_>) -> String {
        heartbeat_line(&beat)
    }

    fn the_line_for_a_stage(stage: WaitStage) -> String {
        the_line_of(a_beat_in(&stage))
    }

    #[test]
    fn a_line_begins_with_the_greppable_prefix_and_how_long_the_wait_has_lasted() {
        let line = the_line_for_a_stage(WaitStage::warming_the_crate_index());

        assert!(line.starts_with("still waiting (2m00s)"), "{line}");
    }

    #[test]
    fn a_line_names_the_warm_up_stage() {
        let line = the_line_for_a_stage(WaitStage::warming_the_crate_index());

        assert!(line.contains("warming the crate index"), "{line}");
    }

    #[test]
    fn a_line_names_the_type_inference_stage_with_its_anchor() {
        let line = the_line_for_a_stage(WaitStage::type_inference_at("src/lib.rs", 1));

        assert!(line.contains("type inference at src/lib.rs:1"), "{line}");
    }

    #[test]
    fn a_line_names_the_assist_stage_with_its_anchor() {
        let line = the_line_for_a_stage(WaitStage::an_answer_for_the_assist(
            "extract_module",
            "src/lib.rs",
            7,
        ));

        assert!(
            line.contains("an answer for the extract_module assist at src/lib.rs:7"),
            "{line}"
        );
    }

    #[test]
    fn a_line_names_the_outline_stage() {
        let line = the_line_for_a_stage(WaitStage::the_outline_of("src/lib.rs"));

        assert!(line.contains("the outline of src/lib.rs"), "{line}");
    }

    #[test]
    fn a_line_names_the_symbol_stage() {
        let line = the_line_for_a_stage(WaitStage::locating("foo", "src/lib.rs"));

        assert!(line.contains("locating foo in src/lib.rs"), "{line}");
    }

    #[test]
    fn a_line_names_the_request_stage() {
        let line = the_line_for_a_stage(WaitStage::a_request_in_flight("textDocument/hover"));

        assert!(
            line.contains("a textDocument/hover request in flight"),
            "{line}"
        );
    }

    #[test]
    fn a_line_says_the_server_has_said_nothing_yet_before_its_first_line() {
        let stage = WaitStage::warming_the_crate_index();
        let line = the_line_of(Beat {
            last_words: None,
            furthest: None,
            ..a_beat_in(&stage)
        });

        assert!(line.contains("the server has said nothing yet"), "{line}");
    }

    #[test]
    fn a_line_quotes_the_servers_last_words_and_the_furthest_phase() {
        let line = the_line_for_a_stage(WaitStage::warming_the_crate_index());

        assert!(
            line.contains("\"working: build script num-bigint run\""),
            "{line}"
        );
        assert!(line.contains("furthest: Roots Scanned 100%"), "{line}");
    }

    #[test]
    fn a_line_says_a_loading_server_is_still_loading() {
        let line = the_line_for_a_stage(WaitStage::warming_the_crate_index());

        assert!(line.contains("is still loading"), "{line}");
    }

    #[test]
    fn a_line_says_a_server_that_never_said_it_is_loading_has_not_said_it_is_ready() {
        let stage = WaitStage::warming_the_crate_index();
        let line = the_line_of(Beat {
            loading: false,
            ..a_beat_in(&stage)
        });

        assert!(line.contains("has not said it is ready"), "{line}");
    }

    #[test]
    fn a_line_says_how_long_the_servers_words_have_been_unchanged_at_zero_seconds() {
        let stage = WaitStage::warming_the_crate_index();
        let line = the_line_of(Beat {
            quiet_for: Duration::ZERO,
            ..a_beat_in(&stage)
        });

        assert!(line.contains("unchanged for 0s"), "{line}");
    }

    #[test]
    fn a_line_says_how_long_the_servers_words_have_been_unchanged_below_a_minute() {
        let stage = WaitStage::warming_the_crate_index();
        let line = the_line_of(Beat {
            quiet_for: 59 * SECOND,
            ..a_beat_in(&stage)
        });

        assert!(line.contains("unchanged for 59s"), "{line}");
    }

    #[test]
    fn a_line_says_how_long_the_servers_words_have_been_unchanged_past_a_minute() {
        let stage = WaitStage::warming_the_crate_index();
        let line = the_line_of(Beat {
            quiet_for: 62 * SECOND,
            ..a_beat_in(&stage)
        });

        assert!(line.contains("unchanged for 1m02s"), "{line}");
    }

    #[test]
    fn a_line_names_a_self_started_server_by_its_pid() {
        let stage = WaitStage::warming_the_crate_index();
        let line = the_line_of(Beat {
            on: WaitedOn::OwnServer { pid: 4242 },
            ..a_beat_in(&stage)
        });

        assert!(line.contains("rust-analyzer (pid 4242)"), "{line}");
    }

    #[test]
    fn a_line_names_a_shared_server_as_behind_a_shared_client() {
        let line = the_line_for_a_stage(WaitStage::warming_the_crate_index());

        assert!(
            line.contains("rust-analyzer behind a shared client"),
            "{line}"
        );
    }

    #[test]
    fn a_wait_is_not_due_a_beat_before_the_heartbeat_has_passed() {
        let began = Instant::now();
        let mut waiting = Waiting::begin(
            WaitStage::warming_the_crate_index(),
            Duration::from_secs(30),
            began,
        );

        assert!(!waiting.beat_due(began + Duration::from_secs(29)));
    }

    #[test]
    fn a_wait_is_due_a_beat_once_the_heartbeat_has_passed_and_again_one_heartbeat_later() {
        let began = Instant::now();
        let mut waiting = Waiting::begin(
            WaitStage::warming_the_crate_index(),
            Duration::from_secs(30),
            began,
        );

        let first = waiting.beat_due(began + Duration::from_secs(31));
        let straight_after = waiting.beat_due(began + Duration::from_secs(32));
        let second = waiting.beat_due(began + Duration::from_secs(62));

        assert_eq!((first, straight_after, second), (true, false, true));
    }
}

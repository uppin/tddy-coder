//! What rust-analyzer says about its own progress, folded into one account.
//!
//! Split out of `backends/rust.rs`, which is past its size budget.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde_json::Value;

/// What a progress line calls a phase the server never gave a title.
const UNTITLED_PHASE: &str = "working";

/// The `$/progress` kind that opens a phase — its title arrives only with it.
const BEGIN: &str = "begin";

/// What a message says of a server that has reported nothing, in place of its words or its phase.
pub(super) const NOTHING_REPORTED: &str = "nothing reported";

/// The least time between two lines shown for one progress token.
///
/// A cold load reports hundreds of notifications per phase — a percent at a time, or one message
/// per crate with no percentage at all — and a console printing each buries the handful of lines
/// that matter. Limiting by token, here, covers every title the server uses and every front end,
/// where matching a title in a front end covered only the title its author had seen. A `begin` and
/// a phase reaching 100% are shown regardless.
pub const PROGRESS_INTERVAL: Duration = Duration::from_secs(2);

/// One work-done progress token the server has in flight: begun, not yet ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivePhase {
    /// The title its `begin` gave; `working` when the `begin` was never seen.
    pub title: String,
    /// The latest message it reported; empty when it has given none.
    pub message: String,
    /// The highest percentage it has reported; `None` when it has reported none.
    pub percentage: Option<u64>,
}

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
    /// When a line was last shown for each token, which [`PROGRESS_INTERVAL`] is measured from.
    /// Per token, for the reason the titles are.
    shown_at: HashMap<String, Instant>,
    /// Whether every distinct report is returned, with no [`PROGRESS_INTERVAL`] between them. For a
    /// consumer that forwards the phases as data — the daemon's warm stream, whose clients read the
    /// percentages — as opposed to one that prints them, which the interval exists for.
    unthrottled: bool,
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
    /// The phase the server reported on most recently, and the highest percentage *that* phase
    /// reached — what a progress display names as "what is happening now". `furthest` cannot stand
    /// in for it: an early phase that finished at 100% would outrank every later one.
    current: Option<(u64, String)>,
    /// The highest percentage each in-flight progress token has reached, so the current phase's
    /// number never goes backwards when a sub-step arrives without one. Per token, for the reason
    /// the titles are.
    reached_by_token: HashMap<String, u64>,
    /// Every token in flight, in the order it began, with what it has said so far. Ordered — not
    /// keyed like the fields above — because a display lists the phases in the order the server
    /// started them.
    in_flight: Vec<(String, ActivePhase)>,
    /// Tokens that were in flight when the reader feeding this chatter lost sight of some
    /// notifications, and have not been heard from since: their `end` may have been among the lost
    /// ones. Dropped from [`Self::in_flight`] once the server says it is quiescent (see
    /// [`Self::lost_sight`]).
    unconfirmed: HashSet<String>,
    /// When the server last said anything *new*: a different progress line, a flip of its
    /// quiescence, or a change of its health.
    ///
    /// A repeat of the line it already said, and a notification the throttle suppressed with the
    /// same text, are not news. What [`Self::quiet_for`] is measured from — and the figure that
    /// separates a slow server from a stuck one in a wait's heartbeat.
    changed_at: Option<Instant>,
}

impl ServerChatter {
    /// A chatter that returns every distinct report instead of one per [`PROGRESS_INTERVAL`] per
    /// token. The default throttles, because its lines are printed; a stream of structured events
    /// is not read a line at a time, and a client that wanted the 50% it was never sent cannot ask.
    pub fn unthrottled() -> ServerChatter {
        ServerChatter {
            unthrottled: true,
            ..ServerChatter::default()
        }
    }

    /// Fold one server-sent message in, and return the line worth printing for it.
    ///
    /// A message that answers a request carries no `method`, which is what keeps every result out
    /// of the progress stream without having to know the ids in flight.
    pub fn absorb(&mut self, message: &Value) -> Option<String> {
        self.absorb_at(message, Instant::now())
    }

    /// [`Self::absorb`] with the clock supplied: how long ago a progress line was shown decides
    /// whether the next one is, so a test has to be able to say what time it is.
    pub fn absorb_at(&mut self, message: &Value, now: Instant) -> Option<String> {
        match message.get("method").and_then(Value::as_str)? {
            "$/progress" => self.progress_at(message.get("params")?, now),
            "experimental/serverStatus" => {
                let params = message.get("params")?;
                let mut changed = false;
                if let Some(health) = params.get("health").and_then(Value::as_str) {
                    let said = params
                        .get("message")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let now_health = Some((health.to_string(), said));
                    changed |= self.health != now_health;
                    self.health = now_health;
                }
                let quiescent = params.get("quiescent").and_then(Value::as_bool)?;
                changed |= self.quiescent != quiescent;
                self.quiescent = quiescent;
                self.reported_status = true;
                if quiescent {
                    self.forget_unconfirmed_phases();
                }
                if changed {
                    self.changed_at = Some(now);
                }
                None
            }
            _ => None,
        }
    }

    /// Fold one `$/progress` notification in.
    ///
    /// The title arrives only with `begin`, so it is held and reused for the `report` lines that
    /// follow it — without that, a report reads as a bare percentage with nothing to attach it to.
    /// Every notification updates what the timeout message and the readiness checks read
    /// (`last`, `furthest`); only what is *returned for display* is limited. A `begin` is always
    /// shown, a `report` only when [`PROGRESS_INTERVAL`] has passed since the last line shown for
    /// its token, or when it reaches 100% — once, because lines are also deduplicated on the phase
    /// and its percentage (or, with none, on the line itself), the server reporting one
    /// notification per *file* scanned with a different absolute path each.
    ///
    /// The clock is supplied by [`Self::absorb_at`], so how long ago a line was shown is something a
    /// test can state.
    fn progress_at(&mut self, params: &Value, now: Instant) -> Option<String> {
        let token = token_key(params.get("token")?);
        let value = params.get("value")?;
        let kind = value.get("kind").and_then(Value::as_str)?;
        self.unconfirmed.remove(&token);
        match kind {
            BEGIN => {
                if let Some(title) = value.get("title").and_then(Value::as_str) {
                    self.titles.insert(token.clone(), title.to_string());
                }
            }
            "end" => {
                self.in_flight.retain(|(key, _)| *key != token);
                self.titles.remove(&token);
                self.reached_by_token.remove(&token);
                self.shown.remove(&token);
                self.shown_at.remove(&token);
                return None;
            }
            _ => {}
        }

        let title = self.titles.get(&token).cloned();
        let line = progress_line(title.as_deref(), value);
        if self.last.as_deref() != Some(line.as_str()) {
            self.changed_at = Some(now);
        }
        self.last = Some(line.clone());

        let percentage = value.get("percentage").and_then(Value::as_u64);
        let phase = title.as_deref().unwrap_or(UNTITLED_PHASE);
        if let Some(percentage) = percentage {
            self.reached(percentage, phase);
        }
        self.reported_on(&token, percentage, phase);
        self.track_in_flight(&token, kind, phase, value);

        let key = match percentage {
            Some(percentage) => percentage.to_string(),
            None => line.clone(),
        };
        if self.shown.get(&token) == Some(&key) {
            return None;
        }
        let due = self.unthrottled
            || self
                .shown_at
                .get(&token)
                .is_none_or(|at| now.duration_since(*at) >= PROGRESS_INTERVAL);
        if kind != BEGIN && percentage.is_none_or(|pct| pct < 100) && !due {
            return None;
        }
        self.shown.insert(token.clone(), key);
        self.shown_at.insert(token, now);
        Some(line)
    }

    /// Make `phase` the current one, carrying the highest percentage its token has reached.
    fn reported_on(&mut self, token: &str, percentage: Option<u64>, phase: &str) {
        let reached = self.reached_by_token.entry(token.to_string()).or_insert(0);
        if let Some(percentage) = percentage {
            *reached = (*reached).max(percentage);
        }
        self.current = Some((*reached, phase.to_string()));
    }

    /// Fold one `begin` or `report` into the token's active phase, opening it on its first sight.
    ///
    /// A `begin` starts the phase afresh; a `report` keeps the latest message it gave and the
    /// highest percentage the token has reached, so a sub-step without a number does not erase it.
    fn track_in_flight(&mut self, token: &str, kind: &str, phase: &str, value: &Value) {
        let position = match self.in_flight.iter().position(|(key, _)| key == token) {
            Some(position) if kind != BEGIN => position,
            found => {
                if let Some(position) = found {
                    self.in_flight.remove(position);
                }
                self.in_flight.push((
                    token.to_string(),
                    ActivePhase {
                        title: phase.to_string(),
                        message: String::new(),
                        percentage: None,
                    },
                ));
                self.in_flight.len() - 1
            }
        };
        let active = &mut self.in_flight[position].1;
        if let Some(message) = value.get("message").and_then(Value::as_str) {
            active.message = message.to_string();
        }
        if let Some(percentage) = value.get("percentage").and_then(Value::as_u64) {
            active.percentage = Some(active.percentage.map_or(percentage, |p| p.max(percentage)));
        }
    }

    /// Drop every phase whose `end` may have been lost, now that the server says it has nothing
    /// left to do. Their titles are kept: a token that was in fact still running reappears under
    /// its own name the next time it reports.
    fn forget_unconfirmed_phases(&mut self) {
        let unconfirmed = std::mem::take(&mut self.unconfirmed);
        self.in_flight
            .retain(|(token, _)| !unconfirmed.contains(token));
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
            .unwrap_or_else(|| NOTHING_REPORTED.to_string());
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

    /// How long the server has said nothing *new*, as of `now`: the time since it last reported a
    /// different progress line, flipped its quiescence or changed its health.
    ///
    /// A repeat of the line it already said, and a notification the throttle suppressed with the
    /// same text, are not news — which is what makes this the figure that separates a slow server
    /// from a stuck one in a wait's heartbeat. Measured against the clock the caller supplies, like
    /// [`Self::absorb_at`], so a test can say what time it is.
    pub fn quiet_for(&self, now: Instant) -> std::time::Duration {
        match self.changed_at {
            Some(changed_at) => now.saturating_duration_since(changed_at),
            // Nothing has ever been said, so there is nothing to have been quiet since.
            None => std::time::Duration::ZERO,
        }
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

    /// The phase the server reported on most recently, and the highest percentage it has reached
    /// (0 while it has reported none).
    ///
    /// What a progress display names, as opposed to [`Self::furthest`]: rust-analyzer finishes
    /// "Roots Scanned" at 100% early and then spends minutes in later phases, which `furthest`
    /// would hide behind that first 100%.
    pub fn current(&self) -> Option<(u64, &str)> {
        self.current
            .as_ref()
            .map(|(percentage, phase)| (*percentage, phase.as_str()))
    }

    /// Every phase the server has in flight — begun and not yet ended — in the order it began them.
    ///
    /// rust-analyzer runs several at once (`Fetching`, `Roots Scanned`, `Indexing`), and a display
    /// that names only [`Self::current`] hides the others. Folded from every report, shown or
    /// throttled: the throttle decides what is printed, not what is known.
    pub fn active_phases(&self) -> Vec<ActivePhase> {
        self.in_flight
            .iter()
            .map(|(_, active)| active.clone())
            .collect()
    }

    /// The reader feeding this chatter fell behind the server and lost some notifications.
    ///
    /// Any phase in flight may have ended unseen, and without its `end` it would stay in
    /// [`Self::active_phases`] for the server's life. Each such phase is kept until the server
    /// next says it is quiescent — by which point a phase that really ended has stopped reporting —
    /// and then dropped, unless it has reported since the gap, which proves it is still running.
    pub fn lost_sight(&mut self) {
        self.unconfirmed
            .extend(self.in_flight.iter().map(|(token, _)| token.clone()));
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

    fn begin(token: &str, title: &str) -> Value {
        json!({ "token": token, "value": { "kind": "begin", "title": title } })
    }

    fn report(token: &str, message: Option<&str>, percentage: Option<u64>) -> Value {
        let mut value = json!({ "kind": "report" });
        if let Some(message) = message {
            value["message"] = json!(message);
        }
        if let Some(percentage) = percentage {
            value["percentage"] = json!(percentage);
        }
        json!({ "token": token, "value": value })
    }

    fn end(token: &str) -> Value {
        json!({ "token": token, "value": { "kind": "end" } })
    }

    fn server_status(quiescent: bool) -> Value {
        json!({
            "method": "experimental/serverStatus",
            "params": { "health": "ok", "quiescent": quiescent }
        })
    }

    fn at(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    fn phase(title: &str, message: &str, percentage: Option<u64>) -> ActivePhase {
        ActivePhase {
            title: title.to_string(),
            message: message.to_string(),
            percentage,
        }
    }

    #[test]
    fn active_phases_lists_every_token_between_begin_and_end_in_begin_order() {
        // Given a server that began fetching, then indexing, and reported on indexing first
        let (mut chatter, t0) = (ServerChatter::unthrottled(), Instant::now());
        chatter.progress_at(&begin("fetch", "Fetching"), t0);
        chatter.progress_at(&begin("index", "Indexing"), at(t0, 1));
        chatter.progress_at(
            &report("index", Some("120/310 (tddy_core)"), Some(38)),
            at(t0, 2),
        );
        chatter.progress_at(
            &report("fetch", Some("crates.io index"), Some(20)),
            at(t0, 3),
        );

        // When its active phases are read
        let active = chatter.active_phases();

        // Then both are there, in the order they began
        assert_eq!(
            active,
            vec![
                phase("Fetching", "crates.io index", Some(20)),
                phase("Indexing", "120/310 (tddy_core)", Some(38)),
            ]
        );
    }

    #[test]
    fn an_active_phase_keeps_its_highest_percentage_and_latest_message() {
        // Given an indexing phase that reached 40%, then reported a sub-step with no percentage
        let (mut chatter, t0) = (ServerChatter::unthrottled(), Instant::now());
        chatter.progress_at(&begin("index", "Indexing"), t0);
        chatter.progress_at(
            &report("index", Some("124/310 (tddy_core)"), Some(40)),
            at(t0, 1),
        );
        chatter.progress_at(
            &report("index", Some("tddy_desktop (lib)"), None),
            at(t0, 2),
        );

        // When its active phases are read
        let active = chatter.active_phases();

        // Then the phase carries the latest message and the 40% it reached, not nothing
        assert_eq!(
            active,
            vec![phase("Indexing", "tddy_desktop (lib)", Some(40))]
        );
    }

    #[test]
    fn a_report_whose_begin_was_not_seen_is_an_untitled_active_phase() {
        // Given a report on a token whose `begin` arrived before anyone was listening
        let (mut chatter, t0) = (ServerChatter::unthrottled(), Instant::now());
        chatter.progress_at(
            &report("late", Some("build script num-bigint run"), None),
            t0,
        );

        // When its active phases are read
        let active = chatter.active_phases();

        // Then it is there under the untitled phase, with no percentage
        assert_eq!(
            active,
            vec![phase(UNTITLED_PHASE, "build script num-bigint run", None)]
        );
    }

    #[test]
    fn an_ended_token_is_no_longer_active() {
        // Given fetching and indexing in flight, and fetching then ended
        let (mut chatter, t0) = (ServerChatter::unthrottled(), Instant::now());
        chatter.progress_at(&begin("fetch", "Fetching"), t0);
        chatter.progress_at(&begin("index", "Indexing"), at(t0, 1));
        chatter.progress_at(
            &report("index", Some("10/310 (subject)"), Some(3)),
            at(t0, 2),
        );
        chatter.progress_at(&end("fetch"), at(t0, 3));

        // When its active phases are read
        let active = chatter.active_phases();

        // Then only indexing remains
        assert_eq!(active, vec![phase("Indexing", "10/310 (subject)", Some(3))]);
    }

    #[test]
    fn active_phases_fold_reports_the_throttle_does_not_show() {
        // Given a throttled chatter that showed a phase's begin, then received a report within the
        // interval it suppresses
        let (mut chatter, t0) = (ServerChatter::default(), Instant::now());
        chatter.progress_at(&begin("index", "Indexing"), t0);
        let suppressed = chatter.progress_at(
            &report("index", Some("57/310 (subject)"), Some(18)),
            at(t0, 10),
        );

        // When its active phases are read
        let active = chatter.active_phases();

        // Then the report was not shown, but the phase knows where it got to
        assert_eq!(suppressed, None);
        assert_eq!(
            active,
            vec![phase("Indexing", "57/310 (subject)", Some(18))]
        );
    }

    #[test]
    fn a_second_begin_on_a_running_token_restarts_its_phase_in_begin_order() {
        // Given indexing that began, reached 40%, and was followed by fetching
        let (mut chatter, t0) = (ServerChatter::unthrottled(), Instant::now());
        chatter.progress_at(&begin("index", "Indexing"), t0);
        chatter.progress_at(
            &report("index", Some("124/310 (tddy_core)"), Some(40)),
            at(t0, 1),
        );
        chatter.progress_at(&begin("fetch", "Fetching"), at(t0, 2));

        // When the server begins the indexing token again, under a new title
        chatter.progress_at(&begin("index", "Reindexing"), at(t0, 3));

        // Then it is a fresh phase — no message, no percentage — listed after fetching
        assert_eq!(
            chatter.active_phases(),
            vec![phase("Fetching", "", None), phase("Reindexing", "", None)]
        );
    }

    #[test]
    fn an_active_phase_keeps_its_highest_percentage_when_a_lower_one_follows() {
        // Given an indexing phase that reached 60%
        let (mut chatter, t0) = (ServerChatter::unthrottled(), Instant::now());
        chatter.progress_at(&begin("index", "Indexing"), t0);
        chatter.progress_at(
            &report("index", Some("186/310 (tddy_core)"), Some(60)),
            at(t0, 1),
        );

        // When it reports a lower percentage
        chatter.progress_at(
            &report("index", Some("93/310 (tddy_web)"), Some(30)),
            at(t0, 2),
        );

        // Then it keeps the 60% it reached, with the latest message
        assert_eq!(
            chatter.active_phases(),
            vec![phase("Indexing", "93/310 (tddy_web)", Some(60))]
        );
    }

    #[test]
    fn a_phase_whose_end_may_have_been_lost_is_dropped_once_the_server_is_quiescent() {
        // Given fetching in flight when the reader lost sight of some notifications
        let (mut chatter, t0) = (ServerChatter::unthrottled(), Instant::now());
        chatter.progress_at(&begin("fetch", "Fetching"), t0);
        chatter.lost_sight();

        // When the server then says it is quiescent
        chatter.absorb_at(&server_status(true), at(t0, 1));

        // Then fetching is no longer active
        assert_eq!(chatter.active_phases(), vec![]);
    }

    #[test]
    fn a_phase_heard_from_after_a_gap_outlives_quiescence() {
        // Given fetching and indexing in flight across a gap, and indexing reporting after it
        let (mut chatter, t0) = (ServerChatter::unthrottled(), Instant::now());
        chatter.progress_at(&begin("fetch", "Fetching"), t0);
        chatter.progress_at(&begin("index", "Indexing"), at(t0, 1));
        chatter.lost_sight();
        chatter.progress_at(
            &report("index", Some("300/310 (subject)"), Some(97)),
            at(t0, 2),
        );

        // When the server then says it is quiescent
        chatter.absorb_at(&server_status(true), at(t0, 3));

        // Then only the phase that proved itself running is still active
        assert_eq!(
            chatter.active_phases(),
            vec![phase("Indexing", "300/310 (subject)", Some(97))]
        );
    }

    #[test]
    fn a_gap_drops_no_phase_while_the_server_is_still_busy() {
        // Given fetching in flight when the reader lost sight of some notifications
        let (mut chatter, t0) = (ServerChatter::unthrottled(), Instant::now());
        chatter.progress_at(&begin("fetch", "Fetching"), t0);
        chatter.lost_sight();

        // When the server says it is still busy
        chatter.absorb_at(&server_status(false), at(t0, 1));

        // Then fetching is still active: nothing says it ended
        assert_eq!(chatter.active_phases(), vec![phase("Fetching", "", None)]);
    }

    #[test]
    fn shows_one_line_of_a_burst_of_percentages_arriving_inside_one_second() {
        // Given a phase that has begun
        let (mut chatter, t0) = (ServerChatter::default(), Instant::now());
        let begun = chatter.progress_at(&begin("scan", "Roots Scanned"), t0);

        // When 300 reports with distinct messages and rising percentages arrive within a second
        let shown: Vec<String> = (0..300u64)
            .filter_map(|n| {
                let params = report("scan", Some(&format!("/crate/{n}")), Some(n * 99 / 300));
                chatter.progress_at(&params, at(t0, n * 3))
            })
            .collect();

        // Then only the begin line was shown
        assert_eq!(begun.as_deref(), Some("Roots Scanned"));
        assert!(shown.is_empty(), "lines leaked through: {shown:?}");
    }

    #[test]
    fn shows_a_report_once_the_interval_has_passed_since_the_last_line_shown() {
        // Given a phase that has begun and a report suppressed inside the interval
        let (mut chatter, t0) = (ServerChatter::default(), Instant::now());
        chatter.progress_at(&begin("scan", "Roots Scanned"), t0);
        let early = chatter.progress_at(&report("scan", Some("/a"), Some(10)), at(t0, 1999));

        // When a report arrives two seconds after the begin
        let due = chatter.progress_at(&report("scan", Some("/b"), Some(20)), at(t0, 2000));

        // Then the early one was dropped and this one is shown
        assert_eq!(early, None);
        assert_eq!(due.as_deref(), Some("Roots Scanned: /b (20%)"));
    }

    #[test]
    fn measures_the_interval_from_the_last_line_shown_not_the_last_one_received() {
        // Given a report shown at 2s and another dropped at 3s
        let (mut chatter, t0) = (ServerChatter::default(), Instant::now());
        chatter.progress_at(&begin("scan", "Roots Scanned"), t0);
        chatter.progress_at(&report("scan", None, Some(20)), at(t0, 2000));
        chatter.progress_at(&report("scan", None, Some(30)), at(t0, 3000));

        // When a report arrives at 3.9s, under two seconds after the last one shown
        let early = chatter.progress_at(&report("scan", None, Some(40)), at(t0, 3900));
        let due = chatter.progress_at(&report("scan", None, Some(50)), at(t0, 4000));

        // Then it is dropped, and the one at 4s is shown
        assert_eq!(early, None);
        assert_eq!(due.as_deref(), Some("Roots Scanned (50%)"));
    }

    #[test]
    fn always_shows_a_phase_reaching_one_hundred_percent_but_only_once() {
        // Given a phase that has begun
        let (mut chatter, t0) = (ServerChatter::default(), Instant::now());
        chatter.progress_at(&begin("scan", "Roots Scanned"), t0);

        // When it reaches 100% inside the window, and says so again
        let done = chatter.progress_at(&report("scan", None, Some(100)), at(t0, 10));
        let again = chatter.progress_at(&report("scan", None, Some(100)), at(t0, 20));

        // Then the first is shown and the repeat is not
        assert_eq!(done.as_deref(), Some("Roots Scanned (100%)"));
        assert_eq!(again, None);
    }

    #[test]
    fn throttles_two_phases_running_at_once_independently() {
        // Given two phases begun together
        let (mut chatter, t0) = (ServerChatter::default(), Instant::now());
        chatter.progress_at(&begin("deps", "Building compile-time-deps"), t0);
        chatter.progress_at(&begin("fetch", "Fetching"), at(t0, 1500));

        // When each is reported to at 2.0s
        let deps = chatter.progress_at(
            &report("deps", Some("build script a run"), None),
            at(t0, 2000),
        );
        let fetch = chatter.progress_at(
            &report("fetch", Some("cargo metadata: started"), None),
            at(t0, 2000),
        );

        // Then the older phase is due and the newer one is not
        assert_eq!(
            deps.as_deref(),
            Some("Building compile-time-deps: build script a run")
        );
        assert_eq!(fetch, None);
    }

    #[test]
    fn keeps_the_last_line_and_the_furthest_percentage_current_when_a_line_is_suppressed() {
        // Given a phase that has begun
        let (mut chatter, t0) = (ServerChatter::default(), Instant::now());
        chatter.progress_at(&begin("scan", "Roots Scanned"), t0);

        // When reports inside the window are suppressed
        let first = chatter.progress_at(&report("scan", Some("/a"), Some(43)), at(t0, 10));
        let second = chatter.progress_at(&report("scan", Some("/b"), None), at(t0, 20));

        // Then nothing was shown, yet what the timeout message reads kept advancing
        assert_eq!((first, second), (None, None));
        assert_eq!(chatter.last.as_deref(), Some("Roots Scanned: /b"));
        assert_eq!(chatter.furthest(), Some((43, "Roots Scanned")));
    }

    #[test]
    fn forgets_a_phase_clock_when_the_phase_ends() {
        // Given a phase that began, was reported to, and ended
        let (mut chatter, t0) = (ServerChatter::default(), Instant::now());
        chatter.progress_at(&begin("t", "Fetching"), t0);
        chatter.progress_at(
            &report("t", Some("cargo metadata: started"), None),
            at(t0, 2000),
        );
        let ended = chatter.progress_at(&end("t"), at(t0, 2001));

        // When a new phase reuses the token a moment later
        let begun = chatter.progress_at(&begin("t", "Fetching"), at(t0, 2002));

        // Then its begin is shown, as the first of a fresh phase
        assert_eq!(ended, None);
        assert_eq!(begun.as_deref(), Some("Fetching"));
    }

    /// Recorded shape of a cold `cargo check`-driven load: one phase, hundreds of unpercentaged
    /// messages, which the old one-line-per-distinct-line rule printed in full.
    #[test]
    fn shows_at_most_two_lines_of_a_build_script_phase_reporting_every_crate() {
        // Given Building compile-time-deps begun
        let (mut chatter, t0) = (ServerChatter::default(), Instant::now());
        let mut shown: Vec<String> = chatter
            .progress_at(&begin("deps", "Building compile-time-deps"), t0)
            .into_iter()
            .collect();

        // When 200 distinct, percentage-free messages arrive inside one second
        shown.extend((0..200u64).filter_map(|n| {
            let message = format!("build script crate-{n} run");
            chatter.progress_at(&report("deps", Some(&message), None), at(t0, n * 5))
        }));

        // Then at most two lines were shown
        assert!(shown.len() <= 2, "{} lines: {shown:?}", shown.len());
    }

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
    /// The daemon's warm stream forwards phases as data: every percentage the server reported has to
    /// reach its client, however fast they arrive.
    #[test]
    fn an_unthrottled_chatter_returns_every_distinct_percentage_of_a_burst() {
        // Given a phase that reports 25, 50 and 75 percent within one second
        let (mut chatter, t0) = (ServerChatter::unthrottled(), Instant::now());
        let at = |ms| t0 + Duration::from_millis(ms);
        chatter.progress_at(&begin("t", "loading crate graph"), at(0));

        // When each report is absorbed
        let shown: Vec<Option<String>> = [(25, 100), (50, 200), (75, 300)]
            .iter()
            .map(|&(percentage, ms)| {
                chatter.progress_at(&report("t", None, Some(percentage)), at(ms))
            })
            .collect();

        // Then every one of them came back
        assert!(shown.iter().all(Option::is_some), "{shown:?}");
    }

    const A_WHILE: Duration = Duration::from_secs(5);
    const LONGER: Duration = Duration::from_secs(10);

    #[test]
    fn has_been_quiet_since_the_first_progress_line_when_the_server_only_repeats_it() {
        // Given a server that said one thing and then said it again
        let (mut chatter, t0) = (ServerChatter::default(), Instant::now());
        chatter.progress_at(&begin("t", "Building compile-time-deps"), t0);
        chatter.progress_at(&report("t", Some("build script num-bigint run"), None), t0);
        chatter.progress_at(
            &report("t", Some("build script num-bigint run"), None),
            t0 + A_WHILE,
        );

        // Then it has been quiet for as long as it has been saying the same thing
        assert_eq!(chatter.quiet_for(t0 + LONGER), LONGER);
    }

    #[test]
    fn stops_being_quiet_when_the_server_says_a_different_progress_line() {
        // Given a server that said one thing and, five seconds later, another
        let (mut chatter, t0) = (ServerChatter::default(), Instant::now());
        chatter.progress_at(&report("t", Some("build script a run"), None), t0);
        chatter.progress_at(&report("t", Some("build script b run"), None), t0 + A_WHILE);

        // Then it has been quiet since the second
        assert_eq!(chatter.quiet_for(t0 + LONGER), A_WHILE);
    }

    #[test]
    fn stops_being_quiet_when_the_server_flips_its_quiescence() {
        // Given a server that said it was busy and, five seconds later, that it was ready
        let (mut chatter, t0) = (ServerChatter::default(), Instant::now());
        chatter.absorb_at(&a_status(false), t0);
        chatter.absorb_at(&a_status(true), t0 + A_WHILE);

        // Then it has been quiet since the flip
        assert_eq!(chatter.quiet_for(t0 + LONGER), A_WHILE);
    }

    #[test]
    fn stops_being_quiet_when_the_server_changes_its_health() {
        // Given a server that reported itself healthy and, five seconds later, degraded
        let (mut chatter, t0) = (ServerChatter::default(), Instant::now());
        chatter.absorb_at(&a_status_reporting("ok", None), t0);
        chatter.absorb_at(
            &a_status_reporting("warning", Some(BUILD_SCRIPTS_FAILED)),
            t0 + A_WHILE,
        );

        // Then it has been quiet since the change
        assert_eq!(chatter.quiet_for(t0 + LONGER), A_WHILE);
    }

    #[test]
    fn stays_quiet_through_a_notification_the_throttle_suppressed_with_the_same_text() {
        // Given a phase whose second report, a second after the first, repeats its text and is not
        // shown
        let (mut chatter, t0) = (ServerChatter::default(), Instant::now());
        chatter.progress_at(&report("t", Some("build script num-bigint run"), None), t0);
        let shown = chatter.progress_at(
            &report("t", Some("build script num-bigint run"), None),
            t0 + Duration::from_secs(1),
        );

        // Then nothing was shown, and it still has been quiet since the first
        assert_eq!(shown, None);
        assert_eq!(chatter.quiet_for(t0 + LONGER), LONGER);
    }
}

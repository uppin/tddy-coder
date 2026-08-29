//! Acceptance tests: the in-jail registry follows the session's live roster.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md (AC13-AC18, AC20)
//!
//! `tddy-tools --mcp` used to build its `SubagentRegistry` from `TDDY_SUBAGENTS_JSON` — an env var
//! fixed when the jail was spawned. An agent attached at minute forty was therefore uncallable
//! until the session restarted, and one detached was still callable forever. The registry now
//! follows `StreamSessionAgents`, and the env var is only the seed that covers the window before
//! the first frame arrives.
//!
//! Frames are pushed directly rather than served over a socket. The transport is already covered
//! where it is implemented; what is wrong-able *here* is what the registry does with a frame, and
//! a real stream would only make that non-deterministic.

use std::time::Duration;

use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_service::proto::connection::{SessionAgentEntry, SessionAgentRoster};
use tddy_tools::session_agents::{
    ConversationState, LiveAgentRoster, ReconnectPacing, RosterError, RosterStreamOutcome,
};

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A roster entry as the daemon publishes it.
fn an_entry(agent_id: &str) -> SessionAgentEntry {
    let (name, daemon) = agent_id
        .split_once('@')
        .expect("builder was given a qualified agent id");
    SessionAgentEntry {
        agent_id: agent_id.to_string(),
        name: name.to_string(),
        daemon_instance_id: daemon.to_string(),
        label: format!("{name} (local)"),
        model: "qwen2.5-coder:7b".to_string(),
        replaces: Vec::new(),
        tools: vec!["Read".to_string(), "Glob".to_string(), "Grep".to_string()],
        codebase_session_id: String::new(),
        clone_state: 1, // AGENT_CLONE_STATE_LOCAL
        clone_error: String::new(),
        status: 0, // SESSION_AGENT_STATUS_UNSPECIFIED
        last_activity: None,
    }
}

fn an_entry_replacing(agent_id: &str, replaces: &[&str]) -> SessionAgentEntry {
    SessionAgentEntry {
        replaces: replaces.iter().map(|r| r.to_string()).collect(),
        ..an_entry(agent_id)
    }
}

/// A published roster at revision `rev`.
fn a_roster(rev: u64, agents: Vec<SessionAgentEntry>) -> SessionAgentRoster {
    SessionAgentRoster {
        session_id: "1780828020298-roster".to_string(),
        rev,
        agents,
    }
}

/// The seed the jail is spawned with — one def, as `TDDY_SUBAGENTS_JSON` carries it.
fn a_seed_def(name: &str) -> SpecializedAgentDef {
    SpecializedAgentDef {
        name: name.to_string(),
        label: None,
        model: "qwen2.5-coder:7b".to_string(),
        base_url: "http://localhost:11434".to_string(),
        api_key: None,
        system_prompt: None,
        system_prompt_path: None,
        tools: vec![SubagentTool::Read, SubagentTool::Glob, SubagentTool::Grep],
        max_turns: 10,
        replaces: Vec::new(),
    }
}

/// A registry seeded from the spawn env, with no frame applied yet.
fn a_seeded_registry(seed: &[&str]) -> LiveAgentRoster {
    LiveAgentRoster::seeded_from(
        "1780828020298-roster",
        seed.iter().map(|n| a_seed_def(n)).collect(),
        "ws-01",
    )
}

/// One pass of the roster stream that applied `applied` snapshots and was then closed by the far end.
fn a_pass_that_applied(applied: u64) -> RosterStreamOutcome {
    RosterStreamOutcome::closed(applied)
}

/// One pass that applied `applied` snapshots and then ended in an error — a relay that gave up on a
/// quiet stream, a daemon that went away mid-frame.
fn a_pass_that_applied_and_then_broke(applied: u64, failure: &str) -> RosterStreamOutcome {
    RosterStreamOutcome::broke(applied, failure.to_string())
}

/// A follower that has just started, with nothing yet recorded against it.
fn a_follower_that_has_just_started() -> ReconnectPacing {
    ReconnectPacing::default()
}

/// How long a pass lasts when it is served its snapshot and then dies straight away.
fn no_longer_than_it_took_to_be_served() -> Duration {
    Duration::from_millis(40)
}

/// How long a subscription that genuinely worked stays up before something ends it.
fn a_whole_working_subscription() -> Duration {
    Duration::from_secs(11 * 60)
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

fn assert_refused_naming(result: Result<impl std::fmt::Debug, RosterError>, fragment: &str) {
    let error = result.expect_err("the call must be refused");
    assert!(
        error.to_string().contains(fragment),
        "refusal must name '{fragment}', was: {error}"
    );
}

// ---------------------------------------------------------------------------
// AC13 — the seed is only a seed
// ---------------------------------------------------------------------------

/// The first frame replaces the seed wholesale rather than merging with it. A merge would keep a
/// seeded agent alive after the daemon stopped listing it, which is the same silent disagreement
/// the frozen env var caused.
#[test]
fn replaces_the_seeded_registry_with_the_first_roster_it_receives() {
    // Given — spawned with `explorer` in TDDY_SUBAGENTS_JSON
    let registry = a_seeded_registry(&["explorer"]);
    registry
        .resolve(Some("explorer@ws-01"))
        .expect("the seed must be usable before the first frame");

    // When — the daemon's first frame lists a different agent
    registry.apply_snapshot(a_roster(2, vec![an_entry("linter@ws-01")]));

    // Then
    registry
        .resolve(Some("linter@ws-01"))
        .expect("the frame's agent must be resolvable");
    assert_refused_naming(registry.resolve(Some("explorer@ws-01")), "explorer@ws-01");
}

/// A frame older than the one already applied is ignored. Frames are whole snapshots, so applying
/// a stale one would move the registry backwards — and reordering is exactly what a reconnect can
/// produce.
#[test]
fn ignores_a_roster_frame_older_than_the_one_already_applied() {
    // Given
    let registry = a_seeded_registry(&[]);
    registry.apply_snapshot(a_roster(5, vec![an_entry("linter@ws-01")]));

    // When
    registry.apply_snapshot(a_roster(3, vec![an_entry("explorer@ws-01")]));

    // Then
    registry
        .resolve(Some("linter@ws-01"))
        .expect("the newer frame must still be in force");
    assert_refused_naming(registry.resolve(Some("explorer@ws-01")), "explorer@ws-01");
}

// ---------------------------------------------------------------------------
// AC14-AC15 — attach and detach take effect in-process
// ---------------------------------------------------------------------------

/// The headline of live attach: an agent added while the jail is running becomes callable without
/// the process restarting.
#[test]
fn opens_a_conversation_with_an_agent_attached_after_it_started() {
    // Given
    let registry = a_seeded_registry(&[]);
    registry.apply_snapshot(a_roster(1, vec![an_entry("explorer@ws-01")]));

    // When
    registry.apply_snapshot(a_roster(
        2,
        vec![an_entry("explorer@ws-01"), an_entry("linter@ws-02")],
    ));

    // Then
    let conversation = registry
        .open_conversation("linter@ws-02")
        .expect("an agent attached after startup must be callable");
    assert_eq!(
        registry.conversation_state(&conversation),
        ConversationState::Open
    );
}

/// The other half: a detached agent stops being callable, and the refusal names the id so the main
/// agent's next turn can say what happened rather than retrying forever.
#[test]
fn refuses_an_agent_that_was_detached_and_names_the_id() {
    // Given
    let registry = a_seeded_registry(&[]);
    registry.apply_snapshot(a_roster(
        1,
        vec![an_entry("explorer@ws-01"), an_entry("linter@ws-02")],
    ));

    // When
    registry.apply_snapshot(a_roster(2, vec![an_entry("explorer@ws-01")]));

    // Then
    assert_refused_naming(registry.resolve(Some("linter@ws-02")), "linter@ws-02");
}

/// A conversation already open with a detached agent is cancelled, not left hanging. An in-flight
/// `subagent_prompt` that never returns is worse than one that errors: the main agent waits on it.
#[test]
fn cancels_a_conversation_whose_agent_was_detached_underneath_it() {
    // Given
    let registry = a_seeded_registry(&[]);
    registry.apply_snapshot(a_roster(1, vec![an_entry("linter@ws-02")]));
    let conversation = registry
        .open_conversation("linter@ws-02")
        .expect("open a conversation");

    // When
    registry.apply_snapshot(a_roster(2, vec![]));

    // Then
    assert_eq!(
        registry.conversation_state(&conversation),
        ConversationState::Cancelled {
            reason: "agent linter@ws-02 was detached from this session".to_string()
        }
    );
}

/// A conversation with an agent that stayed attached is untouched by someone else's detach.
#[test]
fn leaves_a_conversation_open_when_a_different_agent_is_detached() {
    // Given
    let registry = a_seeded_registry(&[]);
    registry.apply_snapshot(a_roster(
        1,
        vec![an_entry("explorer@ws-01"), an_entry("linter@ws-02")],
    ));
    let conversation = registry
        .open_conversation("explorer@ws-01")
        .expect("open a conversation");

    // When
    registry.apply_snapshot(a_roster(2, vec![an_entry("explorer@ws-01")]));

    // Then
    assert_eq!(
        registry.conversation_state(&conversation),
        ConversationState::Open
    );
}

// ---------------------------------------------------------------------------
// AC16 — the main agent is told its tools changed
// ---------------------------------------------------------------------------

/// Each applied revision produces exactly one MCP `notifications/tools/list_changed`. One per
/// revision, not one per entry: the main agent re-lists once and sees the whole new set.
#[test]
fn announces_exactly_one_tool_list_change_per_roster_revision() {
    // Given
    let registry = a_seeded_registry(&[]);

    // When
    registry.apply_snapshot(a_roster(1, vec![an_entry("explorer@ws-01")]));
    registry.apply_snapshot(a_roster(
        2,
        vec![an_entry("explorer@ws-01"), an_entry("linter@ws-02")],
    ));

    // Then
    assert_eq!(registry.tool_list_change_count(), 2);
}

/// A frame that changes nothing announces nothing. The daemon does not publish a no-op revision,
/// but a reconnect re-delivers the current snapshot — and that must not spam the main agent.
#[test]
fn announces_nothing_when_a_reconnect_redelivers_the_revision_already_applied() {
    // Given
    let registry = a_seeded_registry(&[]);
    registry.apply_snapshot(a_roster(1, vec![an_entry("explorer@ws-01")]));

    // When
    registry.apply_snapshot(a_roster(1, vec![an_entry("explorer@ws-01")]));

    // Then
    assert_eq!(registry.tool_list_change_count(), 1);
}

// ---------------------------------------------------------------------------
// AC17 — a registry that cannot be kept current refuses
// ---------------------------------------------------------------------------

/// Serving the last known roster after the stream dies is the failure this whole design exists to
/// prevent: it answers for detached agents and refuses attached ones, silently. So it refuses
/// instead, and says why.
#[test]
fn refuses_subagent_calls_when_it_cannot_keep_the_roster_current() {
    // Given
    let registry = a_seeded_registry(&[]);
    registry.apply_snapshot(a_roster(1, vec![an_entry("explorer@ws-01")]));

    // When
    registry.mark_unavailable("roster stream closed after 3 reconnect attempts");

    // Then
    assert_refused_naming(
        registry.resolve(Some("explorer@ws-01")),
        "roster stream closed",
    );
}

/// A registry that has never received a frame and cannot open the stream refuses too — it does not
/// keep serving the spawn seed indefinitely.
#[test]
fn refuses_rather_than_serving_the_spawn_seed_forever() {
    // Given
    let registry = a_seeded_registry(&["explorer"]);

    // When
    registry.mark_unavailable("roster stream could not be opened");

    // Then
    assert_refused_naming(
        registry.resolve(Some("explorer@ws-01")),
        "roster stream could not be opened",
    );
}

/// Recovering the stream recovers the registry — an unavailable roster is a state, not a
/// terminal one, and a reconnect that succeeds must not leave the session permanently refusing.
#[test]
fn serves_again_once_the_roster_stream_recovers() {
    // Given
    let registry = a_seeded_registry(&[]);
    registry.mark_unavailable("roster stream closed");

    // When
    registry.apply_snapshot(a_roster(4, vec![an_entry("explorer@ws-01")]));

    // Then
    registry
        .resolve(Some("explorer@ws-01"))
        .expect("a recovered stream must restore service");
}

/// A frame whose revision is too old to apply still proves the stream is being served, so it
/// restores service. Refusing while frames keep arriving is a stream that reads as healthy to the
/// follower — which resets its failure count on every frame — and dead to every caller.
#[test]
fn serves_again_when_a_frame_arrives_that_is_too_old_to_apply() {
    // Given
    let registry = a_seeded_registry(&[]);
    registry.apply_snapshot(a_roster(5, vec![an_entry("explorer@ws-01")]));
    registry.mark_unavailable("roster stream closed after 3 reconnect attempts");

    // When
    registry.apply_snapshot(a_roster(3, vec![an_entry("linter@ws-02")]));

    // Then
    registry
        .resolve(Some("explorer@ws-01"))
        .expect("a frame proving the stream is served must restore service at the applied rev");
}

// ---------------------------------------------------------------------------
// AC17 — what one pass of the stream counts as
// ---------------------------------------------------------------------------

/// A pass that applied a snapshot proved the subscription was being served, so however it ended it
/// is a reconnect rather than a broken setup — the rule the follower already applies when the far end
/// closes cleanly, and the one that has to hold when it ends in an error instead. Counting an error
/// against the budget is what turns three healthy passes into a roster declared unavailable, and with
/// it every subagent call refused.
#[test]
fn does_not_count_a_pass_that_served_a_snapshot_before_breaking_against_the_reconnect_budget() {
    // Given
    let pass = a_pass_that_applied_and_then_broke(2, "roster stream: the peer stopped answering");

    // When / Then
    assert!(
        !pass.counts_as_a_failure(),
        "a pass that applied a snapshot must not count against the reconnect budget"
    );
}

/// A pass that produced nothing at all is the broken setup the budget exists for: nothing is serving
/// this subscription, and retrying forever while answering from the spawn seed is the silent
/// wrongness the whole design refuses.
#[test]
fn counts_a_pass_that_broke_before_any_snapshot_arrived_against_the_reconnect_budget() {
    // Given
    let pass = a_pass_that_applied_and_then_broke(0, "StreamSessionAgents call: unimplemented");

    // When / Then
    assert!(
        pass.counts_as_a_failure(),
        "a pass that applied nothing must count against the reconnect budget"
    );
}

/// The reason travels into the refusal the main agent reads, so it has to be the reason the stream
/// actually gave. "The stream ended" is what a clean close looks like and diagnoses nothing.
#[test]
fn carries_the_reason_the_stream_broke_with_rather_than_a_clean_close() {
    // Given
    let pass = a_pass_that_applied_and_then_broke(0, "StreamSessionAgents call: unimplemented");

    // When / Then
    assert_eq!(pass.reason(), "StreamSessionAgents call: unimplemented");
}

/// A pass the far end closed carries no error, so it says that instead of inventing one.
#[test]
fn reports_a_clean_close_as_the_stream_ending() {
    // Given
    let pass = a_pass_that_applied(4);

    // When / Then
    assert_eq!(pass.reason(), "the stream ended");
}

/// A close before anything arrived says so distinctly: "the stream ended" reads as a subscription
/// that worked and then stopped, when in fact nothing ever served it.
#[test]
fn reports_a_close_before_the_first_snapshot_as_nothing_having_arrived() {
    // Given
    let pass = a_pass_that_applied(0);

    // When / Then
    assert_eq!(
        pass.reason(),
        "the stream ended before any snapshot arrived"
    );
}

// ---------------------------------------------------------------------------
// AC17 — how the follower paces its reconnects
// ---------------------------------------------------------------------------

/// The two questions the follower asks after a pass are not the same question. "Was it served?"
/// decides whether the roster is unavailable; "how fast are we churning?" decides the delay. Driving
/// the delay off the first pinned it at the opening backoff for every failure mode that delivers a
/// frame before dying — and since the daemon publishes the current roster on subscribe, that is
/// every failure mode that gets as far as a subscription. The result was a hot loop opening a fresh
/// subscription on the daemon twice a second, forever, with nothing in the logs saying so.
#[test]
fn escalates_the_backoff_when_each_pass_dies_as_soon_as_it_has_been_served() {
    // Given
    let mut follower = a_follower_that_has_just_started();
    let served_then_died = a_pass_that_applied_and_then_broke(1, "the peer stopped answering");

    // When
    let first = follower.record(&served_then_died, no_longer_than_it_took_to_be_served());
    let second = follower.record(&served_then_died, no_longer_than_it_took_to_be_served());
    let third = follower.record(&served_then_died, no_longer_than_it_took_to_be_served());

    // Then — doubling, not merely rising: a run of subscriptions that each die on arrival is a hot
    // loop, and only a delay that grows as fast as the churn does gets out of it
    assert_eq!(
        (first, second, third),
        (
            Duration::from_secs(1),
            Duration::from_secs(2),
            Duration::from_secs(4)
        ),
        "a pass that dies as soon as it is served must still slow the reconnects down"
    );
}

/// Escalation has to be undone by evidence, and the evidence is a pass that lasted. Otherwise a
/// session that hit a rough patch hours ago is still waiting out the ceiling when its agent attaches.
#[test]
fn reconnects_promptly_again_after_a_pass_that_ran_long_enough_to_be_service() {
    // Given
    let mut follower = a_follower_that_has_just_started();
    let died_at_once = a_pass_that_applied_and_then_broke(1, "the peer stopped answering");
    follower.record(&died_at_once, no_longer_than_it_took_to_be_served());
    follower.record(&died_at_once, no_longer_than_it_took_to_be_served());
    follower.record(&died_at_once, no_longer_than_it_took_to_be_served());

    let a_pass_that_worked = a_pass_that_applied(9);

    // When
    let after_a_working_subscription =
        follower.record(&a_pass_that_worked, a_whole_working_subscription());

    // Then
    let what_a_healthy_follower_waits = a_follower_that_has_just_started()
        .record(&a_pass_that_worked, a_whole_working_subscription());
    assert_eq!(
        after_a_working_subscription, what_a_healthy_follower_waits,
        "a pass that ran long enough to be service must clear the churn the short ones built up"
    );
}

/// Churn is not brokenness. A stream that keeps delivering the roster is answering every call
/// correctly, so however fast it reconnects it must never make the registry refuse — refusing here
/// would take out a working session on the strength of a flaky relay.
#[test]
fn never_declares_the_roster_unavailable_while_the_passes_are_being_served() {
    // Given
    let mut follower = a_follower_that_has_just_started();
    let served_then_died = a_pass_that_applied_and_then_broke(1, "the peer stopped answering");

    // When
    for _ in 0..12 {
        follower.record(&served_then_died, no_longer_than_it_took_to_be_served());
    }

    // Then
    assert!(
        !follower.roster_is_unavailable(),
        "a roster that keeps being delivered must stay callable however often the stream restarts"
    );
}

/// The budget the refusal spends is passes that served nothing at all — the broken setup where
/// answering from the spawn seed instead would be the silent wrongness the design refuses.
#[test]
fn declares_the_roster_unavailable_once_the_passes_stop_serving_anything() {
    // Given
    let mut follower = a_follower_that_has_just_started();
    let served_nothing =
        a_pass_that_applied_and_then_broke(0, "StreamSessionAgents call: unimplemented");

    // When
    follower.record(&served_nothing, no_longer_than_it_took_to_be_served());
    follower.record(&served_nothing, no_longer_than_it_took_to_be_served());
    follower.record(&served_nothing, no_longer_than_it_took_to_be_served());

    // Then
    assert!(
        follower.roster_is_unavailable(),
        "three passes that served nothing must declare the roster unavailable"
    );
    assert_eq!(
        follower.unserved(),
        3,
        "the refusal quotes this count, so it must be the number of unserved passes"
    );
}

/// A pass that served something breaks the unserved run, whatever its duration — the run has to
/// mean "nothing is serving this subscription", not "the last few passes were unlucky".
#[test]
fn a_served_pass_clears_the_unserved_run_it_interrupts() {
    // Given
    let mut follower = a_follower_that_has_just_started();
    let served_nothing =
        a_pass_that_applied_and_then_broke(0, "StreamSessionAgents call: unimplemented");
    follower.record(&served_nothing, no_longer_than_it_took_to_be_served());
    follower.record(&served_nothing, no_longer_than_it_took_to_be_served());

    // When
    follower.record(
        &a_pass_that_applied_and_then_broke(1, "the peer stopped answering"),
        no_longer_than_it_took_to_be_served(),
    );
    follower.record(&served_nothing, no_longer_than_it_took_to_be_served());

    // Then
    assert!(
        !follower.roster_is_unavailable(),
        "one served pass must clear the unserved run, was {}",
        follower.unserved()
    );
}

/// Throttling and authority are the same decision seen from two sides. Once the reconnects are
/// spaced a whole ceiling apart, the next frame is that far away — so answering `resolve` from the
/// last one as if it were current is exactly the silent staleness the roster exists to prevent.
#[test]
fn stops_claiming_the_roster_is_current_once_the_reconnects_are_throttled_to_the_ceiling() {
    // Given
    let mut follower = a_follower_that_has_just_started();
    let served_then_died = a_pass_that_applied_and_then_broke(1, "the peer stopped answering");

    // When
    for _ in 0..12 {
        follower.record(&served_then_died, no_longer_than_it_took_to_be_served());
    }

    // Then
    assert!(
        follower.throttled_to_the_ceiling(),
        "a run of passes that each die as soon as they are served must reach the ceiling"
    );
}

/// The refusal has to lift by itself. A throttled follower whose stream finally holds must go back
/// to answering calls, or one rough patch mutes the session's agents for the rest of the run.
#[test]
fn claims_the_roster_again_once_a_pass_lasts_long_enough_to_be_service() {
    // Given
    let mut follower = a_follower_that_has_just_started();
    let served_then_died = a_pass_that_applied_and_then_broke(1, "the peer stopped answering");
    for _ in 0..12 {
        follower.record(&served_then_died, no_longer_than_it_took_to_be_served());
    }

    // When
    follower.record(&a_pass_that_applied(9), a_whole_working_subscription());

    // Then
    assert!(
        !follower.throttled_to_the_ceiling(),
        "a working subscription must lift the throttle, not just slow its growth"
    );
}

/// A brisk reconnect is not a staleness claim. The follower that reconnects in half a second is
/// following the roster as closely as it ever does, so it must keep answering.
#[test]
fn keeps_claiming_the_roster_while_the_reconnects_are_still_prompt() {
    // Given
    let mut follower = a_follower_that_has_just_started();

    // When
    let after_one_short_pass = follower.record(
        &a_pass_that_applied_and_then_broke(1, "the peer stopped answering"),
        no_longer_than_it_took_to_be_served(),
    );

    // Then
    assert!(
        !follower.throttled_to_the_ceiling(),
        "one short pass waits {after_one_short_pass:?} and must not cost the session its roster"
    );
}

// ---------------------------------------------------------------------------
// AC18 — there is no default agent
// ---------------------------------------------------------------------------

/// With an unbounded roster there is no defensible default. Picking the first entry would make the
/// main agent's choice depend on attach order, which is not something it can see.
#[test]
fn refuses_a_conversation_that_names_no_agent_and_lists_the_ones_it_has() {
    // Given
    let registry = a_seeded_registry(&[]);
    registry.apply_snapshot(a_roster(
        1,
        vec![an_entry("explorer@ws-01"), an_entry("linter@ws-02")],
    ));

    // When
    let result = registry.resolve(None);

    // Then
    let error = result.expect_err("a conversation naming no agent must be refused");
    let message = error.to_string();
    assert!(
        message.contains("explorer@ws-01") && message.contains("linter@ws-02"),
        "the refusal must list the agents that are available, was: {message}"
    );
}

/// An empty roster refuses the same way, and says the roster is empty rather than listing nothing
/// and leaving the main agent to infer it.
#[test]
fn refuses_a_conversation_when_no_agent_is_attached_at_all() {
    // Given
    let registry = a_seeded_registry(&[]);
    registry.apply_snapshot(a_roster(1, vec![]));

    // When
    let result = registry.resolve(None);

    // Then
    assert_refused_naming(result, "no agents are attached");
}

// ---------------------------------------------------------------------------
// AC13 — a seeded def runs only the agent it was seeded for
// ---------------------------------------------------------------------------

/// The seeded def is the material this process runs a local agent's turn loop from, and it stays
/// available after a frame re-lists the same agent — a frame carries no endpoint or credential.
#[test]
fn runs_a_seeded_agent_from_the_def_it_was_spawned_with() {
    // Given
    let registry = a_seeded_registry(&["explorer"]);
    registry.apply_snapshot(a_roster(1, vec![an_entry("explorer@ws-01")]));

    // When
    let entry = registry
        .resolve(Some("explorer@ws-01"))
        .expect("the seeded agent must still be listed");

    // Then
    let def = registry
        .local_def_for(&entry)
        .expect("a seeded agent's def must run its turn loop in-process");
    assert_eq!(def.base_url, "http://localhost:11434");
}

/// An agent attached after spawn under a name a seeded def happens to share is a *different* agent.
/// Answering it from the seed would run someone else's endpoint, model and credential under its id.
#[test]
fn has_no_local_def_for_an_agent_attached_after_spawn_under_a_seeded_name() {
    // Given — seeded with `explorer` owned by ws-01
    let registry = a_seeded_registry(&["explorer"]);

    // When — a second daemon attaches its own agent, also called `explorer`
    registry.apply_snapshot(a_roster(1, vec![an_entry("explorer@ws-02")]));
    let entry = registry
        .resolve(Some("explorer@ws-02"))
        .expect("the attached agent must be listed");

    // Then
    assert!(
        registry.local_def_for(&entry).is_none(),
        "an agent attached after spawn must not inherit a same-named seed's credential"
    );
}

// ---------------------------------------------------------------------------
// AC20 — a replaced tool is refused at the call site
// ---------------------------------------------------------------------------

/// This is what makes live attach enforceable: `--allowedTools` was fixed at spawn, so an agent
/// attached afterwards can only withdraw a tool by having the call refused where it is made.
#[test]
fn refuses_a_replaced_tool_and_names_the_agent_that_serves_it() {
    // Given
    let registry = a_seeded_registry(&[]);
    registry.apply_snapshot(a_roster(
        1,
        vec![an_entry_replacing("explorer@ws-01", &["Grep"])],
    ));

    // When
    let result = registry.check_tool_available("Grep");

    // Then
    let error = result.expect_err("a replaced tool must be refused");
    let message = error.to_string();
    assert!(
        message.contains("explorer@ws-01"),
        "the refusal must name the agent to address instead, was: {message}"
    );
}

/// A tool nobody replaced is unaffected — the refusal is scoped to the roster's union, not to
/// everything the roster's agents happen to bind.
#[test]
fn allows_a_tool_no_attached_agent_replaces() {
    // Given
    let registry = a_seeded_registry(&[]);
    registry.apply_snapshot(a_roster(
        1,
        vec![an_entry_replacing("explorer@ws-01", &["Grep"])],
    ));

    // When / Then
    registry
        .check_tool_available("Read")
        .expect("a tool no agent replaced must stay callable");
}

/// A roster that **was** current and then went stale keeps enforcing its withdrawal: those agents
/// were real, and handing back access the operator withdrew is the worse of the two ways to be
/// wrong.
#[test]
fn keeps_a_tool_withdrawn_when_a_roster_it_did_receive_goes_stale() {
    // Given
    let registry = a_seeded_registry(&[]);
    registry.apply_snapshot(a_roster(
        1,
        vec![an_entry_replacing("explorer@ws-01", &["Grep"])],
    ));

    // When
    registry.mark_unavailable("roster stream closed after 3 reconnect attempts");

    // Then
    assert_refused_naming(registry.check_tool_available("Grep"), "explorer@ws-01");
}

/// A withdrawal must never outlive the reachability of its replacement. When the stream gave up
/// before delivering a single frame, every agent the seed names is refused — so enforcing the
/// seed's `replaces` union would leave the session without `Grep` *and* without the agent that took
/// it over, with no recovery short of a restart.
#[test]
fn stops_withdrawing_a_tool_when_no_roster_frame_ever_arrived() {
    // Given — spawned with an agent that replaces Grep, and the stream never delivered a frame
    let registry = LiveAgentRoster::seeded_from(
        "1780828020298-roster",
        vec![SpecializedAgentDef {
            replaces: vec!["Grep".to_string()],
            ..a_seed_def("explorer")
        }],
        "ws-01",
    );

    // When
    registry.mark_unavailable("the roster stream has no client for this transport");

    // Then
    registry
        .check_tool_available("Grep")
        .expect("a withdrawal whose replacement can never be reached must not be enforced");
}

/// Before the stream has given up, the seed is a roster whose agents *are* addressable — so its
/// withdrawals are enforced, which is what covers the window between spawn and the first frame.
#[test]
fn withdraws_a_tool_the_spawn_seed_replaces_while_the_seed_is_still_addressable() {
    // Given
    let registry = LiveAgentRoster::seeded_from(
        "1780828020298-roster",
        vec![SpecializedAgentDef {
            replaces: vec!["Grep".to_string()],
            ..a_seed_def("explorer")
        }],
        "ws-01",
    );

    // When / Then
    assert_refused_naming(registry.check_tool_available("Grep"), "explorer@ws-01");
}

/// Detaching the agent restores the tool in the same process — no relaunch, which is the whole
/// reason the refusal lives here rather than only in the spawn allowlist.
#[test]
fn allows_a_replaced_tool_again_once_its_agent_is_detached() {
    // Given
    let registry = a_seeded_registry(&[]);
    registry.apply_snapshot(a_roster(
        1,
        vec![an_entry_replacing("explorer@ws-01", &["Grep"])],
    ));
    registry
        .check_tool_available("Grep")
        .expect_err("Grep must be withdrawn while the agent is attached");

    // When
    registry.apply_snapshot(a_roster(2, vec![]));

    // Then
    registry
        .check_tool_available("Grep")
        .expect("detaching the last replacing agent must restore the tool");
}

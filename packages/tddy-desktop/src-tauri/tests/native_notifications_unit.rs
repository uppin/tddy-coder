//! The rules inside the native-notification surface, one at a time: what a notification becomes
//! (`desktop_notice_for`), how a session is addressed (`SessionRoute`), how a body is shown without
//! a subtitle, and when the subscriber stays quiet.
//!
//! The end-to-end behaviour — a published notification becoming a shown one — is
//! `native_notifications_acceptance.rs`; these pin the edges of each rule.

mod support;

use support::{
    a_desktop, a_pending_question_notification, a_pending_question_worded, a_status_report,
    a_status_report_for, an_agent_notification, route_to, this_desktops_context, ShownNotices,
    ANOTHER_SESSION, THE_LABEL, THE_SESSION, THIS_DESKTOP,
};
use tauri::Url;
use tddy_desktop::native_notifications::{
    desktop_notice_for, session_route_from_tap, DesktopNotice, SessionRoute, TappedNotification,
    BODY_LIMIT, EXTRA_HOST, EXTRA_SESSION_ID, TAP_ACTION_ID,
};

// ---------------------------------------------------------------------------
// desktop_notice_for — title, subtitle, body
// ---------------------------------------------------------------------------

#[test]
fn an_agent_notification_is_titled_with_the_agents_own_title() {
    // Given an agent that wrote a title
    let written_by_the_agent = an_agent_notification().titled("Migration applied").built();

    // When it becomes a notice
    let notice = desktop_notice_for(&written_by_the_agent, &this_desktops_context());

    // Then the notice carries that title, under the session's label
    let notice = notice.expect("an agent notification for the operator is shown");
    assert_eq!(
        (notice.title.as_str(), notice.subtitle.as_str()),
        ("Migration applied", THE_LABEL)
    );
}

#[test]
fn a_daemon_worded_notification_is_titled_that_its_session_needs_the_operator() {
    // Given a notification the daemon worded itself
    let needs_input = a_status_report("WaitingForInput");

    // When it becomes a notice
    let notice = desktop_notice_for(&needs_input, &this_desktops_context());

    // Then it is titled by the session's label
    let notice = notice.expect("an attention notification for the operator is shown");
    assert_eq!(notice.title, format!("{THE_LABEL} needs you"));
}

#[test]
fn an_agent_body_is_cut_to_the_body_limit_and_ends_in_an_ellipsis() {
    // Given an agent body far longer than a notification centre shows
    let long_body = "word ".repeat(200);
    let written_by_the_agent = an_agent_notification()
        .with_body_markdown(&long_body)
        .built();

    // When it becomes a notice
    let notice = desktop_notice_for(&written_by_the_agent, &this_desktops_context())
        .expect("an agent notification for the operator is shown");

    // Then the body is exactly the limit long, the cut marked by an ellipsis
    assert_eq!(notice.body.chars().count(), BODY_LIMIT);
    assert!(
        notice.body.ends_with('…'),
        "the cut body does not end in an ellipsis: {}",
        notice.body
    );
}

#[test]
fn a_notice_is_addressed_to_its_session_on_this_desktops_daemon() {
    // Given a notification for a session
    let notification = a_status_report("Done");

    // When it becomes a notice
    let notice = desktop_notice_for(&notification, &this_desktops_context())
        .expect("an attention notification for the operator is shown");

    // Then a click on it would open that session on this desktop
    assert_eq!(notice.route, route_to(THE_SESSION));
}

// ---------------------------------------------------------------------------
// desktop_notice_for — sound
// ---------------------------------------------------------------------------

#[test]
fn a_sound_is_played_when_the_agent_asked_for_one() {
    // Given an agent that asked for an audible alert
    let loud = an_agent_notification().asking_for_a_sound().built();

    // When it becomes a notice
    let notice = desktop_notice_for(&loud, &this_desktops_context());

    // Then the notice plays a sound
    assert!(
        notice.expect("shown").sound,
        "the agent's sound was dropped"
    );
}

#[test]
fn no_sound_is_played_when_the_agent_did_not_ask_for_one() {
    // Given an agent notification that asks for no sound
    let quiet = an_agent_notification().built();

    // When it becomes a notice
    let notice = desktop_notice_for(&quiet, &this_desktops_context());

    // Then the notice is silent
    assert!(!notice.expect("shown").sound, "a sound nobody asked for");
}

#[test]
fn a_sound_is_played_for_a_question_a_session_is_asking() {
    // Given a session asking the operator a question
    let asking = a_pending_question_notification();

    // When it becomes a notice
    let notice = desktop_notice_for(&asking, &this_desktops_context());

    // Then the notice plays a sound, whatever the session's own text says
    assert!(notice.expect("shown").sound, "a pending question is silent");
}

#[test]
fn no_sound_is_played_for_a_session_that_merely_finished() {
    // Given a session that finished its turn
    let finished = a_status_report("Done");

    // When it becomes a notice
    let notice = desktop_notice_for(&finished, &this_desktops_context());

    // Then the notice is silent
    assert!(
        !notice.expect("shown").sound,
        "a finished turn made a noise"
    );
}

// ---------------------------------------------------------------------------
// SessionRoute
// ---------------------------------------------------------------------------

#[test]
fn a_session_route_is_the_dashboards_hash_route_to_the_session_on_its_host() {
    // Given a session on this desktop's daemon
    let route = route_to("s-1");

    // When it is written as the dashboard's hash route
    let fragment = route.fragment();

    // Then it names the session and the host
    assert_eq!(fragment, format!("/sessions/s-1?host={THIS_DESKTOP}"));
}

#[test]
fn a_host_that_is_not_url_safe_is_encoded_in_the_route() {
    // Given a daemon instance whose id carries a space and an ampersand
    let route = SessionRoute {
        session_id: "s-1".to_string(),
        host: "dev box&1".to_string(),
    };

    // When it is written as the dashboard's hash route
    let fragment = route.fragment();

    // Then the host stays one query value
    assert_eq!(fragment, "/sessions/s-1?host=dev+box%261");
}

#[test]
fn a_route_replaces_whatever_page_the_dashboard_was_on() {
    // Given the dashboard on its sign-in callback, with a query and a hash of its own
    let dashboard = Url::parse("http://localhost:5173/auth/callback?code=c&state=s#/old").unwrap();

    // When the session's route is applied to it
    let url = route_to("s-1").url_within(&dashboard);

    // Then it is the app's root on the same origin, with the session's route as the hash
    assert_eq!(
        url.as_str(),
        format!("http://localhost:5173/#/sessions/s-1?host={THIS_DESKTOP}")
    );
}

// ---------------------------------------------------------------------------
// Body without a subtitle
// ---------------------------------------------------------------------------

#[test]
fn the_label_is_the_first_line_of_the_body_a_notification_centre_shows() {
    // Given a notice with a label and a body
    let notice = a_notice_with_body("4 done, one blocked");

    // When it is shown by a centre that has no subtitle
    let body = notice.body_with_label();

    // Then the label comes first, then the body
    assert_eq!(body, format!("{THE_LABEL}\n4 done, one blocked"));
}

#[test]
fn the_label_stands_alone_when_the_notice_has_no_body() {
    // Given a notice with a label and no body
    let notice = a_notice_with_body("");

    // When it is shown by a centre that has no subtitle
    let body = notice.body_with_label();

    // Then only the label is shown, with no empty line after it
    assert_eq!(body, THE_LABEL);
}

fn a_notice_with_body(body: &str) -> DesktopNotice {
    DesktopNotice {
        title: "Build finished".to_string(),
        subtitle: THE_LABEL.to_string(),
        body: body.to_string(),
        sound: false,
        route: route_to(THE_SESSION),
    }
}

// ---------------------------------------------------------------------------
// The subscriber's suppression
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_identical_notice_for_the_same_session_is_shown_once() {
    // Given a desktop, and a session whose hook reports the same wait twice
    let desktop = a_desktop().start();

    // When both reports are published
    desktop.publish(a_status_report("WaitingForInput")).await;
    desktop.publish(a_status_report("WaitingForInput")).await;

    // Then the operator is told once
    desktop.shown().assert_sessions(&[THE_SESSION]);
}

#[tokio::test]
async fn a_notice_that_differs_from_the_previous_one_is_shown_again() {
    // Given a session that waits, finishes, and waits again
    let desktop = a_desktop().start();

    // When each report is published
    desktop.publish(a_status_report("WaitingForInput")).await;
    desktop.publish(a_status_report("Done")).await;
    desktop.publish(a_status_report("WaitingForInput")).await;

    // Then each differs from the one before it, so each is shown
    desktop
        .shown()
        .assert_sessions(&[THE_SESSION, THE_SESSION, THE_SESSION]);
}

#[tokio::test]
async fn the_previous_notice_is_remembered_per_session() {
    // Given two sessions that wait, and the first one reporting its wait again
    let desktop = a_desktop().start();

    // When the reports arrive interleaved
    desktop.publish(a_status_report("WaitingForInput")).await;
    desktop
        .publish(a_status_report_for(ANOTHER_SESSION, "WaitingForInput"))
        .await;
    desktop.publish(a_status_report("WaitingForInput")).await;

    // Then the repeat is recognised although another session's notice came between
    desktop
        .shown()
        .assert_sessions(&[THE_SESSION, ANOTHER_SESSION]);
}

#[tokio::test]
async fn a_question_already_shown_is_not_shown_again_when_its_wording_changes() {
    // Given a session asking one question, reported twice in different words
    let desktop = a_desktop().start();

    // When both reports are published
    desktop
        .publish(a_pending_question_worded("q-1", "asks — Run `cargo test`?"))
        .await;
    desktop
        .publish(a_pending_question_worded(
            "q-1",
            "still asks — Run `cargo test`?",
        ))
        .await;

    // Then the operator is told about the question once
    desktop.shown().assert_sessions(&[THE_SESSION]);
}

#[tokio::test]
async fn a_new_question_from_the_same_session_is_shown() {
    // Given a session that asks one question and then another
    let desktop = a_desktop().start();

    // When both are published
    desktop
        .publish(a_pending_question_worded("q-1", "asks — A?"))
        .await;
    desktop
        .publish(a_pending_question_worded("q-2", "asks — B?"))
        .await;

    // Then each question is shown
    desktop.shown().assert_sessions(&[THE_SESSION, THE_SESSION]);
}

#[tokio::test]
async fn a_window_that_shows_the_session_but_has_no_focus_does_not_suppress_it() {
    // Given the operator's page is on a session, but another application has the focus
    let desktop = a_desktop()
        .with_an_unfocused_window_showing(THE_SESSION)
        .start();

    // When that session needs attention
    desktop.publish(a_status_report("WaitingForInput")).await;

    // Then the operator is told, because they are not looking at it
    desktop.shown().assert_sessions(&[THE_SESSION]);
}

#[tokio::test]
async fn a_focused_window_that_shows_another_session_does_not_suppress_this_one() {
    // Given the operator is looking at one session
    let desktop = a_desktop()
        .with_a_focused_window_showing(ANOTHER_SESSION)
        .start();

    // When a different session needs attention
    desktop.publish(a_status_report("WaitingForInput")).await;

    // Then the operator is told about it
    desktop.shown().assert_sessions(&[THE_SESSION]);
}

// ---------------------------------------------------------------------------
// The tap payload — SessionRoute::tap_extra and session_route_from_tap (`#home-inbox` 11/11)
// ---------------------------------------------------------------------------

/// A session on another host, whose ids carry characters a query string encodes.
fn a_route_with_awkward_ids() -> SessionRoute {
    SessionRoute {
        session_id: THE_SESSION.to_string(),
        host: "studio mac & co".to_string(),
    }
}

fn a_tap_carrying(extra: &[(&str, &str)]) -> TappedNotification {
    TappedNotification {
        action_id: TAP_ACTION_ID.to_string(),
        extra: extra
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect(),
    }
}

#[test]
fn a_notices_tap_payload_names_its_session_and_host_under_the_desktops_keys() {
    // Given the route a notice is shown for
    let route = route_to(THE_SESSION);

    // When its tap payload is built
    let extra = route.tap_extra();

    // Then it carries exactly the session id and the host, under the desktop's own keys
    assert_eq!(
        extra,
        [
            (EXTRA_SESSION_ID.to_string(), THE_SESSION.to_string()),
            (EXTRA_HOST.to_string(), THIS_DESKTOP.to_string()),
        ]
        .into_iter()
        .collect(),
        "the tap payload"
    );
}

#[test]
fn a_route_survives_the_round_trip_through_its_tap_payload_unencoded() {
    // Given a route whose host has a space and an ampersand
    let route = a_route_with_awkward_ids();

    // When a tap carries back the payload it was shown with
    let tapped = TappedNotification {
        action_id: TAP_ACTION_ID.to_string(),
        extra: route.tap_extra(),
    };

    // Then the same route comes back, the ids as they were — encoding is the route fragment's job
    assert_eq!(session_route_from_tap(&tapped), Some(route));
}

#[test]
fn a_tap_on_a_notice_missing_either_key_or_with_an_empty_one_opens_nothing() {
    // Given taps whose payload lacks the host, lacks the session, or names an empty session
    let no_host = a_tap_carrying(&[(EXTRA_SESSION_ID, THE_SESSION)]);
    let no_session = a_tap_carrying(&[(EXTRA_HOST, THIS_DESKTOP)]);
    let empty_session = a_tap_carrying(&[(EXTRA_SESSION_ID, ""), (EXTRA_HOST, THIS_DESKTOP)]);

    // When each is read
    // Then none names a route
    assert_eq!(session_route_from_tap(&no_host), None, "no host");
    assert_eq!(session_route_from_tap(&no_session), None, "no session");
    assert_eq!(
        session_route_from_tap(&empty_session),
        None,
        "an empty session"
    );
}

#[test]
fn an_action_other_than_a_tap_opens_nothing_even_on_a_desktop_notice() {
    // Given an action button pressed on a notice the desktop showed
    let pressed = TappedNotification {
        action_id: "snooze".to_string(),
        extra: route_to(THE_SESSION).tap_extra(),
    };

    // When it is read
    // Then it names no route: only a click on the notice itself opens its session
    assert_eq!(session_route_from_tap(&pressed), None);
}

#[test]
fn a_tap_payload_with_keys_of_its_own_still_maps_to_its_route() {
    // Given a tap whose payload carries a key the desktop did not write, beside its own
    let mut tapped = TappedNotification {
        action_id: TAP_ACTION_ID.to_string(),
        extra: route_to(THE_SESSION).tap_extra(),
    };
    tapped
        .extra
        .insert("platform_key".to_string(), "x".to_string());

    // When it is read
    // Then the extra key is ignored
    assert_eq!(session_route_from_tap(&tapped), Some(route_to(THE_SESSION)));
}

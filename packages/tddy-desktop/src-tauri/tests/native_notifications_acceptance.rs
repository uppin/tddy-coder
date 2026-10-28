//! Tddy Desktop shows a native notification when one of its sessions needs its operator, and a
//! click on it brings the dashboard to that session.
//!
//! The desktop registers a subscriber on its daemon's session-notification bus. Everything the
//! operating system would do is behind two ports — [`NativeNotifier`] shows a notice and
//! [`DashboardWindow`] is the window a click acts on — so these suites run the real subscriber over
//! the real bus with an in-memory notifier and a fake window. An OS notification centre is not
//! something a CI runner can observe; the real one is verified by hand (the changeset's manual
//! verification).
//!
//! PRD: `docs/ft/desktop/1-WIP/PRD-2026-10-25-desktop-notifications.md`.

mod support;

use support::{
    a_dashboard_window_at, a_desktop, a_needs_you_notice_saying, a_status_report,
    a_status_report_for, an_agent_notification, route_to, NotificationOwnedBy, ShownNotices,
    WindowEvent, ANOTHER_SESSION, THE_LABEL, THE_SESSION, THIS_DESKTOP,
};
use tddy_desktop::native_notifications::{open_session_route, DesktopNotice};

// ---------------------------------------------------------------------------
// AC1 — an agent-written notification
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_agent_notification_shows_one_native_notification_with_its_title_label_and_plain_text_body(
) {
    // Given a desktop, and an agent that wrote markdown for its operator
    let desktop = a_desktop().start();
    let written_by_the_agent = an_agent_notification()
        .titled("Build finished")
        .with_body_markdown("**4 done**, one `blocked` — [see](https://example.com)")
        .built();

    // When the daemon publishes it
    desktop.publish(written_by_the_agent).await;

    // Then exactly one native notification appears: the agent's title, the session's label and
    // the body with its markup removed. No sound — the agent did not ask for one.
    desktop.shown().assert_exactly(vec![DesktopNotice {
        title: "Build finished".to_string(),
        subtitle: THE_LABEL.to_string(),
        body: "4 done, one blocked — see".to_string(),
        sound: false,
        route: route_to(THE_SESSION),
    }]);
}

// ---------------------------------------------------------------------------
// AC2 — the daemon's own attention notifications, and what is not one
// ---------------------------------------------------------------------------

#[tokio::test]
async fn finished_and_needs_input_show_their_fixed_text_and_activity_shows_nothing() {
    // Given a desktop, and one session's hook reporting that it needs input, that it is running
    // again, and that it finished its turn
    let desktop = a_desktop().start();
    let needs_input = a_status_report("WaitingForInput");
    let running = a_status_report("Running");
    let finished = a_status_report("Done");

    // When the daemon publishes the three, in the order the hook reported them
    desktop.publish(needs_input.clone()).await;
    desktop.publish(running).await;
    desktop.publish(finished.clone()).await;

    // Then the operator is told twice — each time with the notification's own text — and the
    // activity report between them shows nothing
    desktop.shown().assert_exactly(vec![
        a_needs_you_notice_saying(&needs_input.text),
        a_needs_you_notice_saying(&finished.text),
    ]);
}

// ---------------------------------------------------------------------------
// AC3 — not repeated for the session the operator is already looking at
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_notification_for_the_session_the_focused_window_shows_is_not_repeated_natively() {
    // Given a desktop whose focused window already shows one session
    let desktop = a_desktop()
        .with_a_focused_window_showing(THE_SESSION)
        .start();

    // When that session needs attention, and so does another
    desktop.publish(a_status_report("WaitingForInput")).await;
    desktop
        .publish(a_status_report_for(ANOTHER_SESSION, "WaitingForInput"))
        .await;

    // Then only the other session's notification is shown natively
    desktop.shown().assert_sessions(&[ANOTHER_SESSION]);
}

// ---------------------------------------------------------------------------
// AC5 — only the desktop operator's own sessions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn another_os_users_session_is_never_shown() {
    // Given a desktop, and a daemon that also hosts a session of another OS user
    let desktop = a_desktop().start();
    let someone_elses =
        a_status_report_for(ANOTHER_SESSION, "WaitingForInput").owned_by("someone-else");
    let the_operators = a_status_report("WaitingForInput");

    // When the daemon publishes theirs, and then the operator's own
    desktop.publish(someone_elses).await;
    desktop.publish(the_operators).await;

    // Then only the operator's session is ever shown
    desktop.shown().assert_sessions(&[THE_SESSION]);
}

// ---------------------------------------------------------------------------
// AC4 — a click opens the session
// ---------------------------------------------------------------------------

#[test]
fn a_click_navigates_the_main_window_to_the_sessions_route() {
    // Given the dashboard window, minimised or behind other windows, on the app's own page
    let window = a_dashboard_window_at("tauri://localhost/");

    // When the operator clicks a notification for a session
    open_session_route(&window, &route_to(THE_SESSION)).expect("the click was not handled");

    // Then the window is brought to the front and opens that session on this desktop's host,
    // in that order — navigating a window that is still hidden would leave the operator looking
    // at whatever it showed before
    window.assert_events(vec![
        WindowEvent::BroughtToFront,
        WindowEvent::Navigated(format!(
            "tauri://localhost/#/sessions/{THE_SESSION}?host={THIS_DESKTOP}"
        )),
    ]);
}

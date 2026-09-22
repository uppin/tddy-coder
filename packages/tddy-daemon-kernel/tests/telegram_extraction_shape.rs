//! What `#carve` 7/9 delivers: the Telegram control plane in its own crate, reached through a port.
//!
//! 7,403 lines — 19% of `tddy-session-lifecycle` — are Telegram, and they are its only `teloxide`
//! user. Every outbound edge is acyclic. The single back-edge is one field and one call site, and
//! the type it names is defined in a module that moves, so the port inversion is not optional
//! dressing: without it the cluster cannot leave at all.
//!
//! This is also the one node in the stack with a genuinely **mutual** cluster —
//! `telegram_notifier` ↔ `telegram_session_control` and `telegram_notifier` ↔
//! `telegram_multi_select_shortcuts` — so the leaf-first ordering that lets 5/9 and 6/9 skip cluster
//! support cannot work here.

use std::path::{Path, PathBuf};

use tddy_daemon_kernel::presenter_observer::{
    NoPresenterEventSink, PresenterEventSink, SharedPresenterEventSink,
};
use tddy_service::gen::ServerMessage;

fn package(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the packages directory")
        .join(name)
}

fn source_of(package_name: &str, relative: &str) -> String {
    std::fs::read_to_string(package(package_name).join("src").join(relative)).unwrap_or_default()
}

/// The port is usable as a trait object, which is what the connection service holds.
///
/// A port that cannot be `Arc<dyn _>` is not a port — the whole point is that the service names no
/// concrete type.
#[tokio::test]
async fn the_port_is_a_trait_object_the_service_can_hold() {
    // Given the no-op sink, held the way the connection service holds any sink
    let sink: SharedPresenterEventSink = std::sync::Arc::new(NoPresenterEventSink);

    // When a presenter event of a session reaches it
    let delivered = sink
        .on_presenter_event("session-1", &ServerMessage::default())
        .await;

    // Then it accepts the event — nothing is required of the caller behind the port
    assert!(
        delivered.is_ok(),
        "the no-op sink refused an event: {delivered:?}"
    );
    let _: &dyn PresenterEventSink = sink.as_ref();
}

/// AC1 — `connection_service` names no Telegram symbol.
///
/// `connection_service.rs:141` holds `telegram: Option<Arc<TelegramDaemonHooks>>` today.
#[test]
fn the_connection_service_names_nothing_telegram() {
    // Given the service's own module root
    let text = source_of("tddy-session-lifecycle", "connection_service.rs");
    assert!(
        !text.is_empty(),
        "connection_service.rs could not be read — has the module moved?"
    );

    // Then it holds a port rather than the concrete hooks
    assert!(
        !text.contains("TelegramDaemonHooks"),
        "`connection_service` still names `TelegramDaemonHooks`, so the cluster cannot leave"
    );
}

/// AC1 — and neither does the one call site that consumed the field.
#[test]
fn the_only_consumer_of_the_field_goes_through_the_port() {
    // Given the module holding the single call site
    let text = source_of(
        "tddy-session-lifecycle",
        "connection_service/svc_resolve_tddy_tools_path.rs",
    );

    // Then it no longer reaches into the Telegram subscriber
    assert!(
        !text.contains("telegram_session_subscriber"),
        "the spawn path still calls into `telegram_session_subscriber` directly"
    );
}

/// AC3 — `tddy-session-lifecycle` keeps neither `teloxide` nor a Telegram module.
#[test]
fn the_session_lifecycle_crate_is_free_of_telegram() {
    // Given its manifest
    let manifest = std::fs::read_to_string(package("tddy-session-lifecycle").join("Cargo.toml"))
        .unwrap_or_default();

    // Then the transport is gone
    assert!(
        !manifest.contains("teloxide"),
        "`tddy-session-lifecycle` still declares `teloxide`"
    );

    // And so are the modules
    let source = package("tddy-session-lifecycle").join("src");
    let remaining: Vec<String> = std::fs::read_dir(&source)
        .expect("the source directory")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("telegram_"))
        .collect();
    assert!(
        remaining.is_empty(),
        "these Telegram modules have not moved: {remaining:?}"
    );
}

/// AC4 — the new crate depends on both, and neither depends back.
#[test]
fn the_control_crate_depends_on_the_transport_and_the_lifecycle() {
    // Given the new crate's manifest
    let manifest = std::fs::read_to_string(package("tddy-telegram-control").join("Cargo.toml"))
        .unwrap_or_default();
    assert!(
        !manifest.is_empty(),
        "`packages/tddy-telegram-control` has no manifest — the crate does not exist yet"
    );

    // Then it names both crates it sits between
    for needed in ["tddy-telegram", "tddy-session-lifecycle"] {
        assert!(
            manifest.contains(needed),
            "`tddy-telegram-control` does not depend on `{needed}`"
        );
    }
}

/// AC5 — no module of the split control plane is over budget.
///
/// `telegram_session_control.rs` is 3,980 production lines holding a single 2,634-line `impl` with
/// 57 methods. Seven modules is what makes it readable.
#[test]
fn no_control_module_exceeds_eight_hundred_production_lines() {
    // Given the split modules, wherever the crate now lives
    let source = package("tddy-telegram-control").join("src/telegram_session_control");
    if !source.exists() {
        panic!("`telegram_session_control/` has not been split yet");
    }

    // When each is measured on production lines
    let over: Vec<String> = std::fs::read_dir(&source)
        .expect("the control directory")
        .filter_map(Result::ok)
        .map(|entry| {
            let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
            let lines = text
                .lines()
                .position(|line| line.trim_start().starts_with("#[cfg(test)]"))
                .unwrap_or_else(|| text.lines().count());
            (entry.file_name().to_string_lossy().into_owned(), lines)
        })
        .filter(|(_, lines)| *lines > 800)
        .map(|(name, lines)| format!("{name} at {lines}"))
        .collect();

    // Then none is over
    assert!(over.is_empty(), "over 800 production lines: {over:?}");
}

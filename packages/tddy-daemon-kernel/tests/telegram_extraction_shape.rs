//! What `#carve` 8/11 delivers: the Telegram control plane in its own crate, reached through a port.
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
//!
//! Every file this suite reads is read with `expect`: a negative assertion over a file that could
//! not be read would pass for the wrong reason, so a moved or renamed file fails loudly instead.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tddy_daemon_kernel::presenter_observer::{PresenterEventSink, SharedPresenterEventSink};
use tddy_service::gen::ServerMessage;

/// AC5's ceiling on a split control module, in production lines.
const PRODUCTION_LINE_BUDGET: usize = 800;

fn package(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the packages directory")
        .join(name)
}

fn source_of(package_name: &str, relative: &str) -> String {
    let path = package(package_name).join("src").join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is unreadable: {e} — has it moved?", path.display()))
}

fn manifest_of(package_name: &str) -> toml::Table {
    let path = package(package_name).join("Cargo.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{} is unreadable: {e} — does the crate exist?",
            path.display()
        )
    });
    text.parse::<toml::Table>()
        .unwrap_or_else(|e| panic!("{} is not valid TOML: {e}", path.display()))
}

/// The crate's `[dependencies]` table — what it needs to build, as opposed to what its tests need.
fn dependencies_of(package_name: &str) -> toml::Table {
    manifest_of(package_name)
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .cloned()
        .unwrap_or_else(|| panic!("`{package_name}` has no [dependencies] table"))
}

/// Every dependency table of the crate — normal, dev and build, including the
/// `[target.'cfg(..)'.*]` ones — so a back-edge cannot hide in a less obvious table.
fn every_dependency_table_of(package_name: &str) -> Vec<toml::Table> {
    const KINDS: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];
    let manifest = manifest_of(package_name);
    let top_level = KINDS.iter().filter_map(|kind| manifest.get(*kind));
    let per_target = manifest
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|targets| targets.values())
        .filter_map(toml::Value::as_table)
        .flat_map(|target| KINDS.iter().filter_map(|kind| target.get(*kind)));
    top_level
        .chain(per_target)
        .filter_map(toml::Value::as_table)
        .cloned()
        .collect()
}

fn declares_anywhere(package_name: &str, dependency: &str) -> bool {
    every_dependency_table_of(package_name)
        .iter()
        .any(|table| table.contains_key(dependency))
}

/// Every entry under `dir`, recursively — files and directories alike.
fn entries_under(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{} is unreadable: {e}", dir.display()))
        .map(|entry| entry.expect("a directory entry").path())
        .flat_map(|path| {
            let nested = if path.is_dir() {
                entries_under(&path)
            } else {
                Vec::new()
            };
            std::iter::once(path).chain(nested)
        })
        .collect()
}

/// Lines before the file's unit-test module: the first `#[cfg(test)]` whose next non-blank line
/// opens a `mod`, so a `#[cfg(test)] use …;` near the top does not end the count early.
fn production_lines_of(path: &Path) -> usize {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} is unreadable: {e}", path.display()));
    let lines: Vec<&str> = text.lines().collect();
    let opens_a_test_module = |index: usize| {
        lines[index].trim_start().starts_with("#[cfg(test)]")
            && lines[index + 1..]
                .iter()
                .map(|line| line.trim_start())
                .find(|line| !line.is_empty())
                .is_some_and(|line| line.starts_with("mod ") || line.starts_with("pub mod "))
    };
    (0..lines.len())
        .find(|&index| opens_a_test_module(index))
        .unwrap_or(lines.len())
}

/// A sink that remembers which session each event it was handed belonged to.
#[derive(Default)]
struct RecordingSink {
    sessions: Mutex<Vec<String>>,
}

impl RecordingSink {
    fn sessions_seen(&self) -> Vec<String> {
        self.sessions.lock().expect("the recording lock").clone()
    }
}

#[async_trait]
impl PresenterEventSink for RecordingSink {
    async fn on_presenter_event(
        &self,
        session_id: &str,
        _event: &ServerMessage,
    ) -> anyhow::Result<()> {
        self.sessions
            .lock()
            .expect("the recording lock")
            .push(session_id.to_string());
        Ok(())
    }
}

/// The port is a trait object: an event handed to the `SharedPresenterEventSink` the connection
/// service holds reaches the implementation behind it, which the service never names.
#[tokio::test]
async fn an_event_handed_to_the_shared_port_reaches_the_sink_behind_it() {
    // Given a sink, held the way the connection service holds any sink
    let recorder = Arc::new(RecordingSink::default());
    let sink: SharedPresenterEventSink = recorder.clone();

    // When a presenter event of a workflow session is handed to the port
    let delivered = sink
        .on_presenter_event("a-workflow-session", &ServerMessage::default())
        .await;

    // Then the sink behind it received that session's event
    assert!(
        delivered.is_ok(),
        "the port refused an event: {delivered:?}"
    );
    assert_eq!(
        recorder.sessions_seen(),
        vec!["a-workflow-session".to_string()]
    );
}

/// AC1 — `connection_service` names no Telegram symbol.
///
/// `connection_service.rs` held `telegram: Option<Arc<TelegramDaemonHooks>>` before this node.
#[test]
fn the_connection_service_names_nothing_telegram() {
    // Given the service's own module root
    let text = source_of("tddy-session-lifecycle", "connection_service.rs");

    // Then it does not name the concrete hooks
    assert!(
        !text.contains("TelegramDaemonHooks"),
        "`connection_service` still names `TelegramDaemonHooks`, so the cluster cannot leave"
    );
}

/// AC1 — the field is a `tddy-daemon-kernel` port.
#[test]
fn the_connection_service_holds_the_presenter_event_sink_port() {
    // Given the service's own module root
    let text = source_of("tddy-session-lifecycle", "connection_service.rs");

    // Then it holds the kernel's port
    assert!(
        text.contains("SharedPresenterEventSink"),
        "`connection_service` does not hold a `SharedPresenterEventSink`"
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

/// AC3 — `tddy-session-lifecycle` no longer declares the transport.
#[test]
fn the_session_lifecycle_manifest_names_no_teloxide() {
    // Given every dependency table of its manifest
    // Then none declares `teloxide`
    assert!(
        !declares_anywhere("tddy-session-lifecycle", "teloxide"),
        "`tddy-session-lifecycle` still declares `teloxide`"
    );
}

/// AC3 — and keeps no Telegram module, at any depth of its source tree.
#[test]
fn no_telegram_module_is_left_in_session_lifecycle() {
    // Given everything under its source tree
    let source = package("tddy-session-lifecycle").join("src");

    // When the Telegram-named files and directories are picked out
    let remaining: Vec<String> = entries_under(&source)
        .into_iter()
        .filter(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy())
                .is_some_and(|name| name.starts_with("telegram_") || name == "telegram")
        })
        .map(|path| path.display().to_string())
        .collect();

    // Then there are none
    assert!(
        remaining.is_empty(),
        "these Telegram modules have not moved: {remaining:?}"
    );
}

/// AC4 — the new crate sits between the transport and the lifecycle, depending on both.
#[test]
fn the_control_crate_depends_on_the_transport_and_the_lifecycle() {
    // Given the new crate's [dependencies]
    let dependencies = dependencies_of("tddy-telegram-control");

    // Then it names both crates it sits between
    for needed in ["tddy-telegram", "tddy-session-lifecycle"] {
        assert!(
            dependencies.contains_key(needed),
            "`tddy-telegram-control` does not depend on `{needed}`"
        );
    }
}

/// AC4 — and neither of them depends back, in any dependency table.
#[test]
fn neither_crate_the_control_plane_sits_on_depends_back_on_it() {
    // Given the two crates the control plane depends on
    for below in ["tddy-telegram", "tddy-session-lifecycle"] {
        // Then neither declares it
        assert!(
            !declares_anywhere(below, "tddy-telegram-control"),
            "`{below}` depends back on `tddy-telegram-control`, closing a cycle"
        );
    }
}

/// AC5 — no module of the split control plane is over budget.
///
/// `telegram_session_control.rs` was 3,980 production lines holding a single 2,634-line `impl`
/// with 57 methods. Seven modules is what makes it readable.
#[test]
fn no_control_module_exceeds_eight_hundred_production_lines() {
    // Given the split modules
    let source = package("tddy-telegram-control").join("src/telegram_session_control");
    assert!(
        source.is_dir(),
        "`telegram_session_control/` has not been split yet"
    );

    // When each is measured on production lines
    let over: Vec<String> = entries_under(&source)
        .into_iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .map(|path| (path.display().to_string(), production_lines_of(&path)))
        .filter(|(_, lines)| *lines > PRODUCTION_LINE_BUDGET)
        .map(|(name, lines)| format!("{name} at {lines}"))
        .collect();

    // Then none is over
    assert!(
        over.is_empty(),
        "over {PRODUCTION_LINE_BUDGET} production lines: {over:?}"
    );
}

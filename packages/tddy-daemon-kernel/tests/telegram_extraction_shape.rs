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

// The port's own contract — an event handed to the shared trait object reaches the channel behind
// it — is `operator_channel_acceptance.rs`'s, since the port became `OperatorChannel`.

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

/// AC1 — the field is a `tddy-daemon-kernel` port: the operator channel, since
/// `2026-10-22-telegram-satellite` folded the presenter sink into it.
#[test]
fn the_connection_service_holds_the_operator_channel_port() {
    // Given the service's own module root
    let text = source_of("tddy-session-lifecycle", "connection_service.rs");

    // Then it holds the kernel's port
    assert!(
        text.contains("SharedOperatorChannel"),
        "`connection_service` does not hold a `SharedOperatorChannel`"
    );
}

/// AC1 — and neither does the one call site that consumed the field.
#[test]
fn the_only_consumer_of_the_field_goes_through_the_port() {
    // Given the module holding the single call site
    let text = source_of("tddy-session-split", "svc_resolve_tddy_tools_path.rs");

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

#[test]
fn the_control_module_is_split() {
    // Given the control crate
    let source = package("tddy-telegram-control").join("src/telegram_session_control");

    // Then its session control is a directory of modules
    assert!(
        source.is_dir(),
        "`telegram_session_control/` has not been split yet"
    );
}

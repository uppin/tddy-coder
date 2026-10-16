//! The LiveKit plugin crate holds LiveKit work and nothing else — PRD
//! `docs/ft/daemon/1-WIP/PRD-2026-10-16-livekit-satellite-core-modules.md`.
//!
//! A module with no LiveKit in it, kept here, makes every consumer of it depend on the plugin — and
//! the ratchet would then count that consumer as needing LiveKit when it does not. Read from the
//! crate's own `lib.rs`, the one place a module is declared.

use pretty_assertions::assert_eq;

/// Modules that contain no LiveKit and belong with their consumers.
const LIVEKIT_FREE_MODULES: &[&str] = &["placement", "session_git", "os_user_resolution"];

#[test]
fn the_plugin_declares_only_livekit_work() {
    // Given the plugin's module declarations
    let lib = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"))
        .expect("the plugin's lib.rs is readable");

    // When the LiveKit-free modules are looked for among them
    let declared: Vec<&str> = LIVEKIT_FREE_MODULES
        .iter()
        .copied()
        .filter(|module| lib.contains(&format!("pub mod {module};")))
        .collect();

    // Then none of them is there
    assert_eq!(declared, Vec::<&str>::new());
}

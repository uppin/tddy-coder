//! One libwebrtc runtime for the life of the process.
//!
//! The livekit SDK shares a single [`LkRuntime`] — the native `PeerConnectionFactory` and the three
//! libwebrtc threads under it (network, worker, signaling; the network thread owns an epoll instance
//! and a wake-up pipe) — between every `Room` through a `Weak`, and builds a fresh one whenever the
//! last `Arc` to the previous one is gone. A join that fails *after* its peer connections exist —
//! `wait_pc_connection timed out`, which is what a daemon sees when its LiveKit server advertises
//! ICE candidates it cannot reach — is dropped by the SDK's engine without `close()`. The session's
//! tasks then keep those peer connections, and with them the native runtime they were created on,
//! alive for good. The next join finds no live `LkRuntime`, builds a new runtime, and the process is
//! one epoll instance, two pipe ends and three threads heavier every time the join loop passes
//! through a moment with no room connected: 337 runtimes against a 1024-descriptor limit in forty
//! hours (incident 2026-11-13).
//!
//! Pinning one runtime for the process makes every later join reuse it, so the descriptor and
//! thread counts stay flat whatever the join loop does. It does not free the sessions the engine
//! leaks — that is the SDK's defect, recorded in
//! `docs/dev/todo/2026-11-13-livekit-failed-join-leaks-its-session.md`.

use std::sync::{Arc, OnceLock};

use livekit::rtc_engine::lk_runtime::LkRuntime;

static PINNED: OnceLock<Arc<LkRuntime>> = OnceLock::new();

/// Keep the process's libwebrtc runtime alive for the rest of the process.
///
/// Idempotent, and free after the first call. Every room-connecting entry point in this crate
/// calls it before `Room::connect`; a process that wants the runtime up before its first join may
/// call it itself.
pub fn pin_webrtc_runtime() {
    PINNED.get_or_init(LkRuntime::instance);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pinned_runtime_is_the_one_every_later_join_resolves_to() {
        // Given
        pin_webrtc_runtime();
        let pinned = PINNED.get().expect("pinned");

        // When
        let resolved_by_a_join = LkRuntime::instance();

        // Then
        assert!(
            Arc::ptr_eq(pinned, &resolved_by_a_join),
            "a join resolved a runtime other than the pinned one"
        );
    }

    #[test]
    fn pinning_again_keeps_the_same_runtime() {
        // Given
        pin_webrtc_runtime();
        let first = Arc::clone(PINNED.get().expect("pinned"));

        // When
        pin_webrtc_runtime();

        // Then
        assert!(
            Arc::ptr_eq(&first, PINNED.get().expect("pinned")),
            "a second pin replaced the runtime"
        );
    }
}

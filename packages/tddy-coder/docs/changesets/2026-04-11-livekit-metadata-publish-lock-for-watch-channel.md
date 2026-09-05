# 2026-04-11 — LiveKit metadata publish lock for watch channel

**Type:** Feature

`spawn_local_participant_metadata_watcher` receives `participant.metadata_publish_lock()` after `connect`; `run_with_reconnect_metadata` / `connect` pass `projects_registry_dir: None` until registry path wiring. Feature: [livekit-participant-owned-projects.md](../../../../docs/ft/web/livekit-participant-owned-projects.md). (tddy-coder)

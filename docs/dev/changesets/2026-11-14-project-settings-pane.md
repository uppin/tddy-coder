# 2026-11-14 — A project's session settings behind a gear

**Type:** Feature

A gear on each project header in the session drawer opens the project's session settings at `#/sessions/project/<id>/settings[?host=]`: main's setup and Replace main (moved off the pinned entry) and per-project orchestrator defaults (model, effort, provider account) stored on the project record per host via `SetProjectOrchestratorDefaults`, which the orchestrator composer pre-fills. The old project-home routes open the same pane in place, and the Projects card's Set up main link names the host whose row it read.

This resolves the backlog entry "A Projects card's Set up main link reflects one host; the setup pane acts on another" (2026-10-24-project-card-setup-link-ignores-card-host): the settings route carries `?host=` and the card link passes its row's host, pinned by `ProjectHomeSetupLinkAcceptance`.

File-length gate (step 3.5): `SessionsDrawerScreen.tsx` grew 581 → 625 production lines and `coordinate_handlers.rs` grew 564 → 566 while already over budget; both are stack-shared files other stack PRs also modify, so neither is split here — the splits are deferred to a follow-up branch after the stack lands. `livekit_peer_discovery.rs` was already over budget and did not grow (1402 → 1402); generated `project_pb.ts` is excluded from the gate.

The shared orchestrator-sessions PRD is wrapped here, the last node to touch it: orchestrator sessions are documented in [Project concept](../../ft/daemon/project-concept.md) and [Session drawer](../../ft/web/session-drawer.md), with main replaced from the settings pane.

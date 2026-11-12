# A project's session settings behind a gear — PRD

**Date**: 2026-11-10
**PRD Type**: Enhancement
**Stack**: `#orchestrators` 11/11 (`feature/orchestrators/project-settings-pane`)

## Affected Features

- **Primary**: [Session drawer § Pinned main entry](../session-drawer.md#pinned-main-entry) — setup and
  Replace main move into the settings pane
- [Session drawer § Main variant](../session-drawer.md#main-variant)
- [Project concept § Data model](../../daemon/project-concept.md#data-model) — orchestrator defaults on
  the project record

## Summary

Each project header gets a **gear** (`sessions-drawer-project-settings-<id>`) beside its "+". It opens
the project's **session settings** at `#/sessions/project/<id>/settings`:

- **Main** — today's setup form (*Set up main*), or, when main is set up, its model and status and
  **Replace main**. The pinned main entry's own Replace action moves here; clicking an unconfigured
  main still opens this pane.
- **New orchestrators** — defaults the "+" composer starts with: model, effort, provider account.
  Saved per project on the host (`SetProjectOrchestratorDefaults`, listed on `ProjectEntry`); the
  composer pre-fills them, and the operator can still change them per start.

`#/sessions/project-home/<id>` and `/replace` keep working and open the same pane. The pane's route
carries the host it acts on (`?host=<daemon_instance_id>`; absent = the drawer's selected host), and the
Projects card's **Set up main / Open main** link names the host whose row it read, so the card and the
pane can no longer disagree about which host's main they mean.

## Proposed Changes

### What's Changing
- Gear button; `ProjectSettingsPane` hosting `ProjectHomeSetupPane` and a defaults form.
- `ProjectData.orchestrator_defaults { model, effort, provider_account_id }`; RPC
  `SetProjectOrchestratorDefaults`; `ProjectEntry.orchestrator_defaults`.
- The composer reads them.
- `?host=` on the settings route; the Projects card link passes its host.

### What's Staying the Same
- `EnsureProjectHomeSession` and its outcomes; the pinned entry's click behaviour.

## Acceptance Criteria
- [ ] The gear opens the settings pane for that project on the selected host
- [ ] Main can be set up and replaced from the pane; the old routes open it
- [ ] Saved defaults pre-fill the composer; a cleared default falls back to the agent's
- [ ] Defaults are per host and survive a daemon restart
- [ ] A Projects card link opens the pane for the host its row names, even when the drawer has another host selected

## References
- [Session drawer](../session-drawer.md), [Project concept](../../daemon/project-concept.md)

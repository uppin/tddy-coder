# LiveKit integration status in the settings screen - PRD

**Date**: 2026-10-16
**PRD Type**: Enhancement
**Stack**: #satellite 9/11. Umbrella PRD on branch `satellite-1-extension-ports` (#623).

## Affected Features

- **Primary Feature**: [Daemon settings](../../daemon/daemon-settings.md). The settings screen's
  LiveKit block shows the integration's live state.

## Summary

The daemon settings screen shows, beside the LiveKit block's heading, what the daemon's LiveKit
integration is doing right now. That comes from `DaemonConfigService.GetIntegrationStatus` (added in
#623). There are five states: **Off** (disabled), **Missing: <fields>** (misconfigured),
**Connecting…**, **Connected**, and **Unreachable: <error>**, each with how long it has been in that
state. The status refreshes while the screen is open, so an operator who saves a URL sees whether the
daemon reached it. It needs no restart and no log reading.

## Background

The defect that started this stack was invisible from the UI. LiveKit was configured with a server
that did not answer, so sessions' terminals never attached, and the only evidence was a repeating
line in `~/.tddy/logs/daemon`. LiveKit is now a satellite (#623), and its being down no longer
breaks local work. That makes it more important to show the state, because nothing else will.

## Proposed Changes

### What's Changing
- The LiveKit block in `DaemonSettingsScreen.tsx` gains a status line
  (`data-testid="daemon-settings-livekit-status"`) rendered from the `livekit` entry of
  `GetIntegrationStatus`.
- Polling: once on mount, then every 5 s while the screen is mounted, and once right after a
  successful save.
- A daemon that does not implement `GetIntegrationStatus` (`Unimplemented`) shows no status line
  rather than an error. That is an older daemon, not a broken one.

### What's Staying the Same
- The form, its validation, save, and the restart-required banner.
- Other screens. The host chrome is out of scope.

## Impact Analysis
- **Technical:** one screen component, one Cypress component spec, and a backend stub for the new
  RPC in the settings driver.
- **User:** the operator can see the integration's state.

## Implementation Plan
1. A status presentation function (state to text, pure, unit tested).
2. Fetching and polling in the screen.
3. A Cypress component spec covering each state, with `mountWithRpc`-style stubs.

## Success Criteria
- Each of the five states renders its text, and an unreachable state names the error.
- Saving triggers a refresh. The poll stops on unmount.

## Out of Scope
- Showing status outside the settings screen.
- Status for integrations other than LiveKit (the board supports them; there are none yet).

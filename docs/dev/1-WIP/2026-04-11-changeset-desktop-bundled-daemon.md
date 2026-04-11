# Changeset: Desktop bundled tddy-daemon

**Date**: 2026-04-11  
**Status**: Complete  
**Type**: Feature

## Plan (implementation reference)

Embedded PRD and technical plan: bundle `tddy-daemon` (macOS v1), start on Electrobun launch with `TDDY_DAEMON_CONFIG`, stop on exit; `prebuild` copies release binary to `resources/bin/`; `electrobun` `build.copy` includes it in the app bundle.

**PRD:** [docs/ft/1-WIP/PRD-2026-04-11-desktop-bundled-daemon.md](../../ft/1-WIP/PRD-2026-04-11-desktop-bundled-daemon.md)

## Affected Packages

- **tddy-desktop**: [README.md](../../packages/tddy-desktop/README.md), [electrobun.config.ts](../../packages/tddy-desktop/electrobun.config.ts), new `embedded-daemon` module, scripts, tests, [docs/changesets.md](../../packages/tddy-desktop/docs/changesets.md)

## Related Feature Documentation

- [PRD-2026-04-11-desktop-bundled-daemon.md](../../ft/1-WIP/PRD-2026-04-11-desktop-bundled-daemon.md)

## Summary

Ship `tddy-daemon` next to the desktop shell, spawn it when the main process starts (if config + binary resolve), tear down on process exit.

## Milestones

- [x] Changeset opened
- [x] Path resolution + spawn module + index wiring
- [x] `build-daemon` script + `prebuild` + `build.copy`
- [x] Bun tests for resolution helpers
- [x] README + package changeset bullet

## Acceptance Tests (automated)

- Unit tests for `resolveDaemonConfigPath` / `resolveDaemonBinaryPath` with controlled env and temp paths.

## Validation Notes

- Manual: `TDDY_DAEMON_CONFIG=... bun run desktop:dev` (with daemon binary available) and verify `/api/config`; quit app and confirm port released.

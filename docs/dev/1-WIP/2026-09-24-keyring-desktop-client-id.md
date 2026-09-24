# Changeset: Tddy Desktop ships its GitHub OAuth App client id

**Date**: 2026-09-24
**Status**: ✅ Complete
**Type**: Configuration
**Stack**: `#keyring` 2/9 (#509), M8 — the milestone its main changeset deferred to the developer

## Affected Packages

- **tddy-desktop**: `desktop.yaml.production` (repo root) and
  [config-resolution-and-install.md](../../../packages/tddy-desktop/docs/config-resolution-and-install.md)

## Related Feature Documentation

- [Tddy Desktop](../../ft/desktop/tddy-desktop-tauri.md) § *What a fresh install still lacks*

## Summary

The developer supplied the Tddy Desktop OAuth App's public client id, `Ov23lioH6CfiaZR8ESr5`. It is
rendered into `desktop.yaml.production` as `github: { client_id }` with **no** `client_secret`, so a
fresh `./install --desktop` registers the device flow out of the box. The template's identity
comments describe the device flow and first-login enrolment instead of a secret and a hand-written
`users:` row.

## Prerequisites

### ⚠ NARROWED, KEPT — [`2026-09-18-desktop-install-configures-no-identity.md`](../todo/2026-09-18-desktop-install-configures-no-identity.md)

Its item 1 (render the client id) is done here; item 2 — a fresh `./install --desktop` reaching a
signed-in dashboard with no file edited by hand, plus the live-API `refresh_token` check — remains
the developer's, and the entry is deleted when they confirm.

## Scope

- [x] `desktop.yaml.production`: `github.client_id`, no secret; identity comments rewritten
- [x] Verified: `tddy-e2e` `install_script` desktop tests 6/6; the rendered template (placeholders
      substituted) loads as a `DaemonConfig` and `github_auth_flow` returns `Device`, no secret
      (temporary probe, not kept)
- [x] Package and feature docs: *What a fresh install still lacks*
- [ ] ~~A fresh install signs in end to end~~ — deferred to the developer (needs the real app and a
      GitHub approval); tracked in the narrowed backlog entry

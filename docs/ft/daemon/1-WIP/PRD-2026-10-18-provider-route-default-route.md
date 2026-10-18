# Default provider account per harness - PRD

**Date**: 2026-10-18
**PRD Type**: Enhancement
**Stack**: #provider-route 5/9 (base `feature/provider-route/tool-route`)

## Affected Features

- **Primary Feature**: [Accounts screen](../../web/accounts-screen.md) — mark an account as the default for
  one or more harnesses.
- **Related Feature**: [Claude CLI session](../claude-cli-session.md) and tool sessions — a start that names
  no account uses the person's default for that harness.

## Summary

A person marks a provider account as their **default** for a harness (e.g. "Claude Code → Meta"). A
session start that names no account then uses that default; the Create-session pane pre-selects it and
the operator can still pick "Anthropic (built in)" or another account for that one session.

## Background

The interview asked for a daemon/user default level. With accounts in the vault there is no daemon-held
key, so a **daemon-wide** default cannot carry a key; the default is **per person**, stored with the
person's own account records, and synced with them.

## Proposed Changes

### What's Changing
- `SetProviderAccountDefaults(account_id, harnesses[])` on `AccountsService`; `AccountSummary` lists the
  harnesses an account is default for. At most one default per harness per person; setting one clears
  the previous holder. A harness the account cannot drive (per the route matrix) is refused.
- Session start feeds `RouteSources.user_default` from the owner's vault; an explicit session choice —
  including the explicit "built in" choice — wins.
- Accounts screen: per-account "Default for…" control. Create-session pane pre-selects the default.

### What's Staying the Same
Sessions for people with no defaults; precedence session > agent definition > default (defined in node 2).

## Acceptance Criteria
- [ ] With Meta as Claude Code default, a claude-cli start without an account runs on Meta.
- [ ] Choosing "Anthropic (built in)" for one session overrides the default.
- [ ] Setting a second Claude Code default clears the first.
- [ ] Marking a Baseten account default for Claude Code is refused with the reason.
- [ ] The default survives a daemon restart (stored in the vault record).

## Out of Scope
A daemon-wide default (no daemon-held key exists); agent definitions (node 8).

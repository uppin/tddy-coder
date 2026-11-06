# One host coding-agent registry — PRD

**Date**: 2026-11-06
**PRD type**: Requirements change (existing feature)
**Stack**: `#agent-tools` 22/28 — branch `feature/agent-tools/host-registry`

## Affected Features

- [daemon-settings.md](../daemon-settings.md) — `<tddy home>/tools/*.yaml` agent overrides
- [telegram-session-control.md](../telegram-session-control.md) — agents Telegram can start
- [session-agent-roster.md](../session-agent-roster.md) — the agent catalog and model listing

## Summary

A daemon loads its coding-agent registry (built-in specs plus `<tddy home>/tools/` overrides)
**once** at start, refuses to start if that load fails, and every consumer uses that same registry:
the session host, Telegram and the model catalog. An operator's model-list override is honoured by
the model probe. The PTY sandbox no longer offers ACP-only agents it cannot run.

## Background

- [2026-11-06-telegram-loads-the-coding-agent-registry-a-third-time.md](../../../dev/todo/2026-11-06-telegram-loads-the-coding-agent-registry-a-third-time.md):
  the registry is loaded three times. Telegram logs a failed load and continues with an empty
  registry, which is a silent fallback.
- `DaemonSessionHost.coding_agents` is an `Option`. Readers turn `None` into an empty registry in
  three places, which is a fallback the stack rule forbids ("no production fallback without coding
  agents").
- The model probe runs `tddy-tools list-models`, which lists from the **built-in** specs. An operator
  override of an agent's models in `tools/<agent>.yaml` is ignored.
- [2026-11-04-choose-agent-offers-acp-agents-to-pty-sandbox.md](../../../dev/todo/2026-11-04-choose-agent-offers-acp-agents-to-pty-sandbox.md):
  the PTY sandbox's agent choice includes `claude-acp` and `codex-acp`, which have no PTY `run`.

## Proposed Changes

### What's Changing

- The daemon loads the host registry once. A load error stops start-up with the error, so there is
  no empty-registry branch anywhere.
- `DaemonSessionHost` always holds a registry. Every former `None` reader uses it.
- The model probe lists models from the host registry, including operator overrides.
- The PTY sandbox offers, and accepts, only agents whose spec has a PTY `run`. Choosing an ACP-only
  agent there is refused, naming the agents it can run.

### What's Staying the Same

- Spec loading itself: built-ins plus `tools/` overrides, and the same YAML.
- A host that legitimately has no coding agents still refuses agent starts with `FailedPrecondition`.

## Impact Analysis

### Technical Impact

`tddy-daemon` (runtime wiring), `tddy-session-lifecycle` (host field and builders, ~10 test files),
`tddy-daemon-rpc` (catalog probe), `tddy-tools` / `tddy-tools-cli` (`list-models` reads a tools dir),
`tddy-sandbox-app` (`choose_agent`).

### User Impact

A broken `tools/*.yaml` stops the daemon with a message instead of quietly hiding Telegram's
agents. Model overrides show up in the Create-session model list.

## Implementation Plan

1. One fallible load in runtime wiring; pass the `Arc` to the host, Telegram and reserved ids.
2. Make `coding_agents` non-optional; delete the empty fallbacks.
3. The probe passes the host's tools dir to `list-models`.
4. Filter `choose_agent` to PTY-runnable specs.

## Acceptance Criteria

- [ ] Daemon start with an unparseable `tools/<agent>.yaml` fails, and the error names the file.
- [ ] Telegram, the session host and the reserved-ids step share one registry instance (one load).
- [ ] No production path turns a missing registry into an empty one.
- [ ] With `tools/claude.yaml` overriding the curated models, `ListAgents`/model listing returns the override.
- [ ] The PTY sandbox's agent list excludes `claude-acp` and `codex-acp`; choosing one is refused with the runnable list.

## References

### Affected Features (Complete List)

- `docs/ft/daemon/daemon-settings.md`
- `docs/ft/daemon/telegram-session-control.md`
- `docs/ft/daemon/session-agent-roster.md`

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-11-06-one-host-coding-agent-registry.md`

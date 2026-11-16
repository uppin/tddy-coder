# An orchestrator starts a session on a derived host - PRD

**Date**: 2026-11-16
**PRD Type**: Feature
**Stack**: `#remote-shell 11/11` — branch `feature/remote-shell/orchestrator-target-start`, base `feature/remote-shell/remote-checkout-root`

## Affected Features

- **Project concept — project-home tools**: [project-concept.md](../project-concept.md) — `DescribeSessionStartOptions` lists derived hosts; `StartSession` takes a target

## Summary

An orchestrator agent can start a session on an SSH host or a Coder workspace: `DescribeSessionStartOptions` lists the derived hosts the daemon reaches (kind, parent, usable, reason), `start_session` takes `target` — a derived-host id — which the daemon resolves for the **caller** (the calling session's token names whose Coder account is read) into the parent daemon and the host's `SessionTarget`; an unknown id is refused naming the listed ones, an unusable host with its reason. The parity test moves `target` from withheld to offered, and the tool's text says a wildcard `Host` entry is not an alias.

## Background

`#remote-shell 3/11`'s CI fix withheld `StartSessionRequest.target` from the orchestrator's start tool
(`INTERNAL_START_FIELDS`) because offering it needed a design: the tool must name a target the daemon
can resolve, and a Coder target needs an account. `docs/dev/todo/2026-11-16-orchestrator-start-tool-cannot-start-on-a-target.md`
records it. On the first real run an orchestrator asked for `remote-dev`, saw no such daemon or alias
(`coder config-ssh` writes a wildcard `Host coder.*`, which `ListSshConfigHosts` rightly does not list),
guessed `coder.remote-dev` as `ssh_config_host` and ran into the legacy exec-host mode.

The tool call carries the calling session's token; `StartSession` already resolves a Coder target's
account from the caller's vault (`shell_for_session_target(target, subject, credentials)`). What is
missing is the way to name the host.

## Proposed Changes

### What's Changing

- `StartOptions.derived_hosts: Vec<DerivedHostChoice>` (`id`, `label`, `kind`, `parent_daemon_instance_id`, `usable`, `unusable_reason`) and `derived_hosts_unavailable`, filled from `HostService::list_derived_hosts` on the described host
- `StartSessionArgs.target: String` — a derived-host id from the options; empty for the host itself
- the daemon resolves `target` by listing the parent's derived hosts for the caller: the request gets `daemon_instance_id = parent` and `target = the host's target`; unknown → `INVALID_ARGUMENT` naming the listed ids; unusable → `FAILED_PRECONDITION` with the reason
- `INTERNAL_START_FIELDS` loses `target`; `offered_start_fields` gains it; the schema's `ssh_config_host` text says a wildcard `Host` is not an alias and Coder workspaces are listed under `derived_hosts`
- the TODO entry is resolved and deleted at wrap

### What's Staying the Same

- `ssh_config_host` keeps its meaning (agent local, tools remote) and its list
- how the daemon starts on a target once the request names one (`#remote-shell 3/11`, `7/11`)
- the web form, which already sends `target`

## Impact Analysis

### Technical Impact

- `tddy-tool-engine`: `start_options.rs` and `start_session_args.rs` gain the fields; the schema fixture for the web parity (`createSessionRequestParity.cy.ts`) is regenerated
- `tddy-daemon-rpc`: `project_home_start_options.rs` lists derived hosts; the start handler resolves `target` before building the request
- `tddy-host-service`: a `resolve_derived_host(subject, parent, id)` helper over the same sources the listing uses

### User Impact

- an orchestrator, a project-home main or a Telegram start can place a session on a Coder workspace or SSH host, like the web form
- an agent that names a host that is not listed is told what is listed

## Implementation Plan

1. `tddy-tool-engine`: fields, schema text, parity move; tests
2. `tddy-host-service`: resolution helper; tests
3. `tddy-daemon-rpc`: start options list derived hosts; the start resolves `target`; acceptance
4. docs: project-concept.md § Start options and § StartSession; delete the TODO at wrap

Tests are written first and run per touched package; the whole-workspace result is read from CI.

## Acceptance Criteria

- [ ] `DescribeSessionStartOptions` lists the described host's derived hosts with kind, parent, usability and reason; an unanswerable list says why
- [ ] `start_session` with `target = <derived host id>` starts on the host's parent with the host's `SessionTarget`
- [ ] An unknown `target` is refused naming the listed ids; an unusable one is refused with its reason
- [ ] A Coder derived host is resolved with the calling session's own account
- [ ] `start_session_args_parity` passes with `target` offered; the tool schema carries `target` and the wildcard note

## References

### Affected Features (Complete List)

- **Project concept — project-home tools**: [project-concept.md](../project-concept.md) — `DescribeSessionStartOptions` lists derived hosts; `StartSession` takes a target

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-11-16-remote-shell-orchestrator-target-start.md` on this branch
- Other PRs of this stack are named by branch in the changeset's `## Dependencies` and `## Successor PRs`

# Changeset: list a host's SSH config Host aliases

**Date**: 2026-09-14
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-09-14-ssh-config-hosts-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Responsibility

This PR owns parsing an OS user's OpenSSH config on a tddy host and publishing the explicit
`Host` aliases: `ListSshConfigHosts` on `host.HostService`, the in-tree parser, and the Hosts
row SSH connections cell. That cell **is** the SSH menu later nodes reuse.

## Boundaries

- Does **not** add `ssh_config_host` on `StartSession` or session metadata (n2).
- Does **not** introduce `LocalShell`/`RemoteShell` or change `execute_tool` (n2).
- Does **not** change `RemoteGitService` (n3) or split codebase-host filtering (n4).
- Does **not** parse config in the browser or add an SSH crate.
- Does **not** change `ListHostKeyCandidates`, ssh-agent probe, or add-key.
- Does **not** execute `ssh` or open a connection.

## Dependencies

This is the stack root. No parent PR.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| — | none | — | — |

## Draft PR contract

The first push of this PR (wave 2), not its deliverable:

- Proto: `ListSshConfigHosts` / `ListSshConfigHostsRequest` / `ListSshConfigHostsResponse` /
  `SshConfigHost` (`alias`) plus a distinguished failure (not an empty `repeated`).
- `list_ssh_config_hosts` (or equivalent) in `tddy-host-service` with `// TODO(ssh-config): implement`.
- Hosts cell component that renders aliases vs empty vs failed.
- Failing tests: parser fixtures, RPC handler (addressing + honesty), Cypress Hosts cell.

## Green wave

**Wave:** 1 of 2
**Greenable independently:** yes — parser and Hosts cell tests inject fixture files / in-memory RPC; they do not need exec tools or split start
**Concurrent with:** `#ssh-exec` 2/4 (exec doubles this list RPC)
**Blocks:** exec and split dropdowns, which consume the list surface

Real dependency edges:

    n1 → n2, n4      n2 → n3, n4

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) this change runs into.

### ⚠ DURING — An unprivileged daemon cannot probe another OS user — [`2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md`](../todo/2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md)

Config is read as the daemon's OS user on supervised installs, same as tooling probes. An
`EPERM` (or any unread) is a **failure**, never an empty alias list. Recorded, not fixed here:
supervisor-brokered read is a deployment decision, not this node's parser.

## Affected Packages

**CRITICAL**: List ALL packages with documentation changes:

- **tddy-host-service**: [host-service.md](../../packages/tddy-host-service/docs/host-service.md) — new RPC and parser
  - [host-tooling-probe.md](../../packages/tddy-host-service/docs/host-tooling-probe.md) — SSH connections vs ssh-agent keys
- **tddy-service**: proto `host.proto` — `ListSshConfigHosts`
- **tddy-web**: [hosts-screen.md](../../packages/tddy-web/docs/hosts-screen.md) — SSH connections cell

## Related Feature Documentation

- [PRD](../../ft/web/1-WIP/PRD-2026-09-14-ssh-config-hosts.md)
- [Hosts screen tooling](../../ft/web/hosts-screen-tooling.md)
- [Hosts screen](../../ft/web/hosts-screen.md)

## Summary

A host can list the explicit OpenSSH `Host` aliases in its OS user's `~/.ssh/config`. The Hosts
row shows them. Session start does not consume the list yet.

## Background

`#ssh-exec` needs a reusable list of SSH destinations per daemon. Key listing is the wrong
surface: it names files, and it treats a missing `config` as silence. This node is the
destinations menu.

## Scope

**High-level deliverables tracking progress throughout development:**

- [ ] **Package Documentation**: Update host-service and hosts-screen docs on wrap
- [ ] **Implementation**: Parser, RPC, Hosts cell
- [ ] **Testing**: All acceptance tests passing
- [ ] **Integration**: Peer-forward listing on a named `daemon_instance_id`
- [ ] **Technical Debt**: Unprivileged-user limit recorded, not absorbed
- [ ] **Code Quality**: Linting and review complete

## Technical Changes

### State A (Current)

`HostService` has `ListHostKeyCandidates` and `GetHostTooling.ssh_agent`. Nothing reads
`~/.ssh/config`. The Hosts SSH cell is keys only (`HostRowSshAgent`).

### State B (Target)

`ListSshConfigHosts` returns explicit aliases for the addressed host's OS user, or a failure
that cannot be mistaken for empty. Hosts shows an SSH connections cell. Parser honors
`Include` and skips wildcard `Host` patterns.

### Delta (What's Changing)

#### tddy-service
- **API**: `ListSshConfigHosts` on `HostService`.

#### tddy-host-service
- **Architecture**: in-tree OpenSSH config alias parser; handler peer-forwards like key listing.
- **API**: `list_ssh_config_hosts` public function used by the handler.

#### tddy-web
- **Implementation**: Hosts SSH connections cell; Cypress distinguishes listed / empty / failed.

## Implementation Milestones

- [ ] Parser lists explicit aliases and skips `Host *` / `?`
- [ ] `Include` is followed (relative to the including file)
- [ ] Unreadable config is a failure; missing/empty is an empty list
- [ ] RPC honours `daemon_instance_id` peer forward
- [ ] Hosts cell renders the three states without collapse

## Testing Plan

### Testing Strategy

**Primary Test Approach:** Unit tests for the parser (fixture files, no live `ssh`); handler
integration against `HostService` with a fake `HostUserFiles`; Cypress component test for the
Hosts cell via `mountWithRpc` + `anInMemoryRpcBackend`.

**Why:** Listing is a pure parse plus one RPC. E2E against a real daemon home directory would
couple CI to the runner's `~/.ssh/config`.

### Testing Options Analysis

#### Option 1: Fixture-file parser + in-memory RPC (primary)
**Test Level**: Unit + Cypress component
**Description**: Parser tests write temp `config` / `Include` files. Handler tests inject
`HostUserFiles`. UI tests mount the cell with canned proto responses.

**Assertions**:
- [ ] **Explicit alias listed, wildcard omitted**
- [ ] **Unreadable config ≠ empty list**
- [ ] **Hosts cell: listed / empty / failed are distinguishable**

**Reliability**: Deterministic temp dirs; no OpenSSH binary.

#### Option 2: Shell out to `ssh -G`
**Rejected:** enumerates one host, not the alias set; needs a live ssh.

### Coverage Requirements

- [ ] Happy path: two explicit Hosts
- [ ] Error: unreadable config
- [ ] Edge: empty, `Include`, wildcards only
- [ ] Integration: `daemon_instance_id` addressing

## Acceptance Tests

### tddy-host-service
- [ ] **Unit**: lists explicit Host aliases and skips wildcard patterns (`packages/tddy-host-service/src/ssh_config.rs`)
- [ ] **Unit**: follows Include and still skips wildcards in included files (`packages/tddy-host-service/src/ssh_config.rs`)
- [ ] **Unit**: unreadable config is a failure, missing config is an empty list (`packages/tddy-host-service/src/ssh_config.rs`)
- [ ] **Integration**: ListSshConfigHosts honours daemon_instance_id and does not collapse a failed read into zero aliases (`packages/tddy-host-service/src/ssh_config_handler_tests.rs`)

### tddy-web
- [ ] **Component**: Hosts SSH connections cell distinguishes aliases listed, none, and could-not-check (`packages/tddy-web/cypress/component/HostsScreenSshConfigAcceptance.cy.tsx`)

## Technical Debt & Production Readiness

- [ ] Supervised daemon may only read its own user's config (see Prerequisites)

## Decisions & Trade-offs

- **In-tree parser, no crate.** Consent not given for a dependency. OpenSSH `Include` + `Host`
  is a small grammar.
- **Failure ≠ empty.** Opposite of `ListHostKeyCandidates`, because empty here selects LocalShell
  in later nodes.
- **Separate RPC, not a GetHostTooling block.** Session dropdown needs a cheap list.

## Refactoring Needed

### From @ft-dev (Acceptance Test Creation)
- [ ] Issue: Description

### From @red (TDD Red Phase)
- [ ] Issue: Description

### From @validate-changes (Change Validation)
- [ ] Issue: Description

### From @validate-tests (Test Quality)
- [ ] Issue: Description

### From @prod-ready (Production Readiness)
- [ ] Issue: Description

### From @analyze-clean-code (Code Quality)
- [ ] Issue: Description

### From @refactor (Completed Refactorings)

## Validation Results

### Change Validation (@validate-changes)

**Last Run**:
**Status**:

## Successor PRs

- `feature/ssh-exec/exec` — session dropdown + RemoteShell
- `feature/ssh-exec/split` — same list, codebase host B

## TODO

- [x] Record initial discovery (`2026-09-14-ssh-config-hosts-initial-discovery.md`)
- [x] Cross-check `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run all tests (`./test`) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs)
- [ ] USER REVIEW — work complete, decide next steps

## References

- Stack slug `#ssh-exec` 1/4
- Draft PR: https://github.com/uppin/tddy-coder/pull/483
- makers-lt `LocalShell`/`RemoteHost` analogue (execution is n2)

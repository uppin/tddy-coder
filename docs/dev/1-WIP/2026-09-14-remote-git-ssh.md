# Changeset: serve remote-git packs through the session's shell

**Date**: 2026-09-14
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-09-14-remote-git-ssh-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Responsibility

This PR owns `RemoteGitService.Serve` executing pack verbs through the Shell n2 publishes, so
an SSH-backed project still packs from T. The git-over-LiveKit client is unchanged.

## Boundaries

- Does **not** parse SSH config (n1) or add create-session / split dropdowns (n2/n4).
- Does **not** change `tddy-remote-git-repo`, proto frames, mint, or verb whitelist.
- Does **not** add git protocol v2 or peer forwarding.
- Does **not** reimplement RemoteShell.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` ssh-config | `ListSshConfigHosts` | unused directly (aliases already chosen at start) | parse config |
| `n2` exec | `Shell` / `RemoteShell`, `ssh_config_host` + remote repo path on session/project | Serve spawns pack verbs through that Shell | change execute_tool, StartSession UI |

## Draft PR contract

The first push of this PR (wave 2), not its deliverable:

- Serve spawn takes a `Shell` (TODO body until green).
- Failing tests: upload-pack with `ssh_config_host` hits the remote path; unset stays local;
  `tddy-remote-git-repo` clone against an SSH-backed fixture; whitelist still refuses other verbs.

## Green wave

**Wave:** 2 of 2
**Greenable independently:** no — tests need n2 RemoteShell **behaviour** (pack bytes from T)
**Concurrent with:** `#ssh-exec` 4/4 (split) after n2 is green
**Blocks:** none in this stack

Real dependency edges:

    n1 ssh-config → n2 exec, n4 split
    n2 exec → n3 remote-git, n4 split

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) this change runs into.

### ⚠ DURING — Remote git repo over LiveKit — deliberate gaps — [`2026-08-15-remote-git-repo-over-livekit-deliberate-gaps.md`](../todo/2026-08-15-remote-git-repo-over-livekit-deliberate-gaps.md)

No peer forwarding, no v2, global stream cap, no idle deadline. This node only changes **where**
pack verbs exec. Do not invent a second git protocol.

## Affected Packages

**CRITICAL**: List ALL packages with documentation changes:

- **tddy-worktree-service**: [remote-git-service.md](../../packages/tddy-worktree-service/docs/remote-git-service.md) — spawn through Shell

## Related Feature Documentation

- [PRD](../../ft/daemon/1-WIP/PRD-2026-09-14-remote-git-ssh.md)
- [remote-git-repo.md](../../ft/daemon/remote-git-repo.md)

## Summary

Serve packs an SSH-backed project from the SSH target. Clients keep using `tddy-remote-git-repo`.

## Background

AC37 clones talk to the facilitating daemon's RemoteGitService. After n2 the bits may not be
on that daemon's disk.

## Scope

**High-level deliverables tracking progress throughout development:**

- [ ] **Package Documentation**: remote-git-service spawn seam
- [ ] **Implementation**: Shell-backed pack spawn
- [ ] **Testing**: All acceptance tests passing
- [ ] **Integration**: real `tddy-remote-git-repo` client against SSH-backed Serve
- [ ] **Technical Debt**: existing remote-git gaps recorded, not absorbed
- [ ] **Code Quality**: Linting and review complete

## Technical Changes

### State A (Current)

Serve spawns local `git upload-pack` / `git receive-pack` on pipes against `main_repo_path`.

### State B (Target)

Same admission; spawn uses session/project Shell. SSH field set → verbs run on T. Unset → local.

### Delta (What's Changing)

#### tddy-worktree-service
- **Implementation**: spawn seam takes `Shell`; remote path when `ssh_config_host` is set.

## Implementation Milestones

- [ ] Serve spawn injected with Shell
- [ ] SSH-backed upload-pack reads T
- [ ] Unset field is local spawn
- [ ] Client clone still works

## Testing Plan

### Testing Strategy

**Primary:** integration — fixture repo on a localhost SSH alias; Serve with RemoteShell;
`tddy-remote-git-repo` clone; compare refs/objects. Unit: whitelist unchanged.

## Acceptance Tests

### tddy-worktree-service
- [ ] **Integration**: Serve upload-pack with ssh_config_host packs the remote repo (`packages/tddy-worktree-service/tests/remote_git_ssh_serve_acceptance.rs`)
- [ ] **Integration**: Serve without ssh_config_host still packs local main_repo_path (`packages/tddy-worktree-service/tests/remote_git_ssh_serve_acceptance.rs`)
- [ ] **Integration**: tddy-remote-git-repo clones an SSH-backed project (`packages/tddy-daemon/tests/remote_git_ssh_clone_acceptance.rs`)

## Technical Debt & Production Readiness

- [ ] Deliberate remote-git gaps unchanged (Prerequisites)

## Decisions & Trade-offs

- **Reuse Serve, do not teach C to OpenSSH to T.** Specialized agents already have
  `tddy-remote-git-repo`. One SSH client (the code-managing daemon).
- **Do not change the client binary.**

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

- `feature/ssh-exec/split` — same `ssh_config_host` on workspace StartSession; does not reimplement Serve

## TODO

- [x] Record initial discovery (`2026-09-14-remote-git-ssh-initial-discovery.md`)
- [x] Cross-check `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code (spawn_pack_verb Remote; serve() wiring pending)
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

- Stack slug `#ssh-exec` 3/4
- Draft PR: https://github.com/uppin/tddy-coder/pull/485

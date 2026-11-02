# Changeset: Start a new tmux session as a session tab

**Created:** 2026-11-02
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 2 — one more terminal kind on n3's spawn path, name validation, a menu item with a name field
**Stack:** `#session-tabs 7/7` — branch `feature/session-tabs/tmux-new`, base `feature/session-tabs/dormant-agent-tab`
**PRD:** [PRD-2026-11-02-tmux-new-session-tab.md](../../ft/web/1-WIP/PRD-2026-11-02-tmux-new-session-tab.md)

## Initial Discovery

[2026-11-02-session-tabs-tmux-new-initial-discovery.md](./2026-11-02-session-tabs-tmux-new-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md](../todo/2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md) | ⚠ During — same as n3: runs through n3's as-user spawn path and is refused where that path is refused |

## Affected Packages

- [ ] [`tddy-terminal-rpc`](../../../packages/tddy-terminal-rpc/README.md): `TerminalKind::TmuxNew`, kind `tmux-new`
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): the new-session argv and name validation
- [ ] [`tddy-coder`](../../../packages/tddy-coder/README.md): refuses kind `tmux-new`
- [ ] [`tddy-web`](../../../packages/tddy-web/README.md): **New tmux session…** with a name field

## Responsibility

- `TerminalKind::TmuxNew { session: String }` from kind `"tmux-new"` plus `tmux_session`.
- `tmux_session_name(raw) -> Result<String, Status>`: trims, refuses empty, and refuses `:` or `.`
  (`invalid_argument`).
- `tmux_new_session_argv(binary, name, worktree) = [binary, "-T", "clipboard", "new-session", "-A", "-s", name, "-c", worktree]`, in `svc_tmux_host.rs`.
- The roster's `TmuxNew` arm, on n3's `start_tmux_terminal`.
- The coder participant refuses `tmux-new` like `tmux`.
- **Web:**
  - `nextTmuxSessionName(base, existing): string` (`src/components/sessions/tmuxSessionName.ts`) —
    `base`, then `base-2`, `base-3`, …;
  - the `NewTerminalMenu` item `sessions-terminal-tab-new-tmux-new` and the inline name input
    `sessions-terminal-tab-new-tmux-name`;
  - `useSessionTerminals.open({kind: "tmux-new", tmuxSession})`.

## Boundaries

- No killing tmux sessions; no windows or layouts.
- No change to the attach path, the list RPC or labels (n3).
- No change to dormant worktree resolution (n1).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n3` tmux-attach | `TerminalKind::Tmux`, `StartTerminalSessionRequest.tmux_session`, `TerminalDescriptor.tmux_session`, `start_tmux_terminal`, `HostTmux`, the menu's tmux section, `useTmuxSessions`, `tmux: <name>` labels | adds a second kind and argv on the same path; the prefill reads `useTmuxSessions` | change the proto, the list RPC, the attach argv or the labels |
| `n1` dormant-tabs | worktree resolution for a stopped session | `-c <worktree>` on a stopped session | change it |

## Draft PR contract

The first push publishes:

- `TerminalKind::TmuxNew`, `tmux_session_name`, `tmux_new_session_argv`, `nextTmuxSessionName` as
  `// TODO(tmux-new): implement`;
- the failing tests below.

## Green wave

**Wave:** 2 of 2
**Greenable independently:** after n3 is green — its acceptance tests spawn through n3's as-user tmux path and need the tmux section in the menu
**Concurrent with:** n6 `dormant-agent-tab`
**Blocks:** nothing

Real dependency edges, as opposed to the branch line:

    n1 → n6      n2 → n6      n3 → n7

## State A (after n3)

- tmux tabs can only attach to existing sessions.

## State B

- **New tmux session…** creates (or attaches to) the named session in the worktree as a tab.

## Delta

### New
- `packages/tddy-web/src/components/sessions/tmuxSessionName.ts`, `tmuxSessionName.test.ts`

### Modified
- `tddy-terminal-rpc` `service/ports.rs`.
- `svc_tmux_host.rs`, `svc_terminal_ports.rs`.
- the `tddy-coder` participant.
- `NewTerminalMenu.tsx`, `useSessionTerminals.ts`.
- Docs, at wrap: `docs/ft/web/session-terminal-tabs.md`.

## Testing Strategy

### Acceptance tests

**`tddy-session-lifecycle`**: `packages/tddy-session-lifecycle/tests/tmux_new_session_acceptance.rs` (private tmux socket, as in n3)

- [ ] `a_tmux_new_tab_creates_the_named_session_in_the_worktree_and_attaches`
- [ ] `stopping_a_tmux_new_tab_leaves_its_session_running`
- [ ] `a_tmux_new_tab_for_an_existing_name_attaches_to_it`
- [ ] `a_tmux_new_tab_with_an_empty_name_is_refused`
- [ ] `a_tmux_new_tab_with_a_colon_in_its_name_is_refused`

**`tddy-web`**: `packages/tddy-web/cypress/component/SessionTerminalTabsTmuxNewAcceptance.cy.tsx`

- [ ] `New tmux session… prefills a name not among the listed sessions`
- [ ] `Enter opens a "tmux: <name>" tab for the typed name`
- [ ] `Escape cancels without starting a terminal`

### Unit / integration tests

- [ ] `tmuxSessionName.test.ts` (bun): the base is free; `-2` when the base is taken; the first gap.
- [ ] `svc_tmux_host.rs`: `tmux_session_name` cases; argv shape.
- [ ] `tddy-terminal-rpc`: `from_request("tmux-new")` mapping.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Create / attach / detach | Real tmux, private socket | Same reasons as n3 |
| Name field | Cypress component | UI |

## Decisions & Trade-offs

- **`-A`** (attach if it exists) avoids a race between the list prefill and creation.
- **Prefill from the session short-id**, so a tmux session is recognisable from a real terminal later.

## TODO

- [x] Record initial discovery (`2026-11-02-session-tabs-tmux-new-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
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
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Refactoring Needed
### From /red (TDD red phase)
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |

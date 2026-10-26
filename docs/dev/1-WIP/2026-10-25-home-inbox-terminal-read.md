# Changeset: Project-home agent reads a session's terminal

**Created:** 2026-10-25
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 3 — four packages, one tool, a new accessor over `#term-snapshot`'s screen model, a widened crate accessor; no proto change
**Stack:** `#home-inbox 2/8` — branch `feature/home-inbox/terminal-read`, base `feature/home-inbox/agent-notify`
**PRD:** [PRD-2026-10-25-home-read-terminal.md](../../ft/daemon/1-WIP/PRD-2026-10-25-home-read-terminal.md)

## Initial Discovery

[2026-10-25-home-inbox-terminal-read-initial-discovery.md](./2026-10-25-home-inbox-terminal-read-initial-discovery.md)

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Record | Verdict |
|---|---|
| `#term-snapshot` #760 (host-snapshot), #762 (jail-size) | ✅ Met — merged 2026-10-25 (`ScreenSnapshot` is at `tddy-task/src/terminal_capture.rs:391` on `master`). Was: ⛔ Blocking (sequencing) — the screen model this node reads exists only after their pass 2/green. Not work for this node |
| `packages/tddy-task/docs/code-issues/complexity-terminal-capture-feed.md` | ⚠ During — `text_grid` is a separate method; do not add branches to `feed` |
| `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-connection-service.md` | ⚠ During — widen `terminal_store` in `svc_terminal_ports.rs`, not in `connection_service.rs` |
| `packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md` | ⚠ During — the tool lives in a new module |
| [2026-09-11-the-coder-terminal-coordinate-serves-seven-of-nine.md](../todo/2026-09-11-the-coder-terminal-coordinate-serves-seven-of-nine.md) | — Unrelated: the tool reads the daemon's store, never the coder participant |

## Affected Packages

- [ ] [`tddy-task`](../../../packages/tddy-task/README.md): `TerminalCapture::text_grid`, `TextGrid`
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): a public terminal-store accessor
- [ ] [`tddy-tool-engine`](../../../packages/tddy-tool-engine/README.md): `ReadSessionTerminal` (new module), catalog/predicate
- [ ] [`tddy-daemon-rpc`](../../../packages/tddy-daemon-rpc/README.md): the port over the store; `run_locally` predicate; **a new crate edge `tddy-daemon-rpc → tddy-terminal-rpc`** (`TerminalSessionStore` / `TerminalSession`, to resolve and lock the terminal) — added at green, not in the contract commit
- [ ] [`tddy-tools`](../../../packages/tddy-tools/README.md): advertised-list pin
- [ ] [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md): `project_home_prompt.md`

## Responsibility

- `tddy_task::{TextGrid { lines: Vec<String>, cursor: GridCursor { row, col, visible }, cols, rows, offset, bracketed_paste: bool }, TerminalCapture::text_grid() -> Option<TextGrid>}`
  — the **active** buffer's visible rows (`Vt::view()`, `trim_end`), `Vt::cursor()`, `Vt::size()`,
  `end_offset`, and the sniffer's bracketed-paste mode; `None` while the model is unsized.
- `tddy_session_lifecycle::…::svc_terminal_ports::DaemonSessionHost::terminal_sessions() -> DaemonTerminalSessionStore` (public; the existing `pub(crate) terminal_store` becomes it or delegates).
- `tddy_tool_engine::project_home_terminal_tools::{READ_SESSION_TERMINAL, is_project_home_terminal_tool, project_home_terminal_tool_defs, ProjectHomeTerminalPort::read_terminal(&SessionTarget) -> anyhow::Result<TextGrid>, execute_project_home_terminal_tool}`.
  `ProjectHomeTerminalPort` is a **supertrait of `ProjectHomePort`** (pass 2 deviation 1): the dispatcher resolves the target through `ProjectHomePort::target_records` and reads through `read_terminal`, from one `&dyn ProjectHomeTerminalPort` (trait upcasting).
- The tool answers `{lines, cursor: {row, col, visible}, cols, rows, offset}`. `bracketed_paste` is on `TextGrid` for `#home-inbox 5/8` and is not in the answer.
- The project-home offered catalog and `is_project_home_allowed_tool` include it.
- `ReadSessionTerminal` is a **target tool** under `#start-parity 1/9`'s generic forward. It takes the
  optional `daemon_instance_id` (`TARGET_DAEMON_INSTANCE_ID`), so a call naming another host's session
  is forwarded whole and runs on that host. The port is only ever served locally.
- `tddy_daemon_rpc::exec_tool::project_home` implements the port over the store (`get_terminal(session_id, "main")`).

## Boundaries

- No input, no lease (`#home-inbox 5/8`).
- No scrollback, colours or attributes; `main` only.
- Does not touch `#term-snapshot`'s `ScreenSnapshot`, `snapshot()`, `snapshot_and_subscribe`, resize or the sniffer's own logic; it only reads them.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `#term-snapshot 1/4` host-snapshot (#760) | `TerminalCapture`'s `avt::Vt` screen model sized to the PTY, `resize`, the widened input-mode sniffer (bracketed paste 2004) | `text_grid` reads the `Vt` and the sniffer's 2004 state under the capture lock | change the model, `snapshot()`, the sniffer's parsing, or the sequence point |
| `#term-snapshot 3/4` jail-size (#762) | jailed terminals' model sized from the runner's size report | a jailed child's grid is readable once sized | add or change the size relay |

| `#start-parity 1/9` remote-targets (#750, merged) | the generic forward of a project-home tool whose arguments carry `daemon_instance_id`, with the caller confirmed at the owning daemon | declares `ReadSessionTerminal` a target tool, so another host's session is read on that host | change the forward, the caller confirmation, or `ListProjectSessions` |

Nothing from `#home-inbox 1/8` is consumed; it sits below only because of the line.

**Shared registration (replan 2026-10-26).** `#start-parity 2/9`–`9/9` all add tools to the same
lists this node edits:

- the offered catalog;
- `is_project_home_allowed_tool`;
- the `read_only.rs` pin;
- `project_home_session_tools_acceptance.rs`;
- `exec_tool_admission.rs`;
- `project_home_prompt.md`.

Each rebase conflicts there mechanically. Resolve it by keeping both sides' tools.

## Draft PR contract

The first push publishes `TextGrid`/`GridCursor`, `TerminalCapture::text_grid` (`// TODO(terminal-read): implement`),
the public store accessor `DaemonSessionHost::terminal_sessions`, the tool module (constant, predicate, def,
port trait, dispatch returning an `is_error` stub), the test support change, and the failing tests below.

It deliberately does **not** register the tool. Registration is wiring, and each piece of it satisfies an
acceptance test on its own (the catalog offer, `PERMISSION_DENIED` for an ordinary caller, the existing
resolver's refusals), so publishing it in the contract commit would make those tests pass or fail for the
wrong reason. Left to green, in this order:

- the tool defs in `project_home_tool_defs_with` (and so the offered catalog);
- `is_project_home_tool`, `is_project_home_target_tool` and `is_project_home_allowed_tool`;
- the `run_locally` and caller-held-elsewhere predicates in `tddy-daemon-rpc`;
- the `DaemonProjectHome` port impl, and `ExecToolRpcHandler::from_host` taking `terminal_sessions()`;
- the pins: `read_only.rs`, `exec_tool_admission.rs`, `project_home_session_tools_acceptance.rs`, `project_home_target_host.rs` (its target-tool list) and `project_home_prompt.md`.

No existing pin test is red on this branch.

## Green wave

**Wave:** 1 of 4
**Greenable independently:** within this stack yes; **not before `#term-snapshot` #760 (and #762 for the
jailed case) are green** — `text_grid` reads their screen model (behaviour).
**Concurrent with:** `#home-inbox 1/8` agent-notify
**Blocks:** `#home-inbox 5/8` terminal-keys (its acceptance test verifies keystrokes by reading the screen)

    n1 → n3, n4, n7      n2 → n5      n3 → n6, n7      n6 → n8      (cross-stack, replan 2026-10-26: #start-parity #750 [merged] → n2, n5, n6, n7, n8 · #755 → n3 · #751, #758 → n6 · #coagents [merged] → n6)

## State A

The home agent has `ListProjectSessions`, `Start…`, `ReadSessionWorktree`, `DiffSessionWorktree` and
seven git tools. No terminal access. After `#term-snapshot`, `TerminalCapture` holds a sized
`avt::Vt` but exposes it only as `snapshot()` bytes. The terminal store is reachable only via a
`pub(crate)` accessor in `tddy-session-lifecycle`; `ExecToolRpcHandler` holds no terminal handle.

## State B

`ReadSessionTerminal(session_id)` resolves the target (same project, not home), gets its `main`
terminal from the daemon store (sandbox first), locks the capture and returns
`{lines, cursor, cols, rows, offset}` as JSON. Refusals are `is_error` results naming why.

## Delta

### New
- `TextGrid`, `GridCursor`, `TerminalCapture::text_grid` (tddy-task)
- `project_home_terminal_tools.rs` (tool-engine); the port impl (daemon-rpc)
### Modified
- `tddy-daemon-rpc` gains the dependency `tddy-terminal-rpc` (an internal crate; green)
- `DaemonSessionHost::terminal_sessions()` (public; was `pub(crate) terminal_store`)
- `tests/support/mod.rs` (`Host::terminals`; shared with `#start-parity`)
- `project_home_offered_catalog`, `is_project_home_allowed_tool`, `run_locally` predicate
- `ExecToolRpcHandler::from_host` takes the terminal store
- Pins: `read_only.rs` test, `project_home_session_tools_acceptance.rs`, `exec_tool_admission.rs`; `project_home_prompt.md`
- `docs/ft/daemon/project-concept.md` § Project-home tools at wrap

## Testing Strategy

### Acceptance tests

**`tddy-daemon-rpc`**: `packages/tddy-daemon-rpc/tests/project_home_terminal_read_acceptance.rs`
- [ ] `a_child_running_a_scripted_tui_shows_its_screen_text_and_cursor_through_read_session_terminal`
- [ ] `a_session_on_another_host_is_read_through_the_forward` (two-host world from `tddy-daemon-rpc/tests/support/mod.rs`)
- [ ] `while_the_child_is_on_the_alternate_screen_the_grid_is_the_alternate_screen`
- [ ] `another_projects_session_the_home_session_and_an_unknown_id_are_each_refused_naming_why`
- [ ] `a_session_without_a_live_terminal_is_refused_with_a_reason`
- [ ] `a_non_project_home_caller_is_refused_as_permission_denied`
- [ ] `the_tool_is_offered_to_a_project_home_session_and_not_to_any_other` (extends the existing catalog test)

### Unit tests

All written and failing (pass 2, 6f); they go green with the implementation.

- [ ] `tddy-task`, `packages/tddy-task/tests/terminal_capture_text_grid.rs` (14): `text_grid` is `None` until the model is sized and then has the PTY's size; printable text; CR/LF; a bare CR; cursor addressing; row trimming; a hidden cursor; alt-screen enter and leave; a wide character; a character split across chunks; the offset; a resize; `bracketed_paste` follows DECSET/DECRST 2004. Fail with "a sized model reads as a text grid" (the stub returns `None`).
- [ ] `tddy-tool-engine`, `packages/tddy-tool-engine/tests/project_home_terminal_tools.rs` (2, over a fake port): the answer is `{lines, cursor, cols, rows, offset}` of the grid; a port that cannot read is refused naming the tool and the port's reason. Fail with "ReadSessionTerminal: not implemented".

Not red, so not written here: the tool def (real code) and the resolver's refusals (existing `session_target` code).

**Wording the acceptance tests pin:** a session with no live terminal is refused with a message containing "no live terminal" and the session id. The resolver's own messages ("belongs to another project", "the project-home session is not a target", "no session") are unchanged.

**Test support.** `packages/tddy-daemon-rpc/tests/support/mod.rs` gains `Host::terminals` (the `CliSessionManager` the daemon's terminals live in), so a test can start a session's child in it. `daemon_over` takes the manager and `host_built_by` takes a config instead of a function pointer; `a_daemon_named` and `a_daemon_as_current_user` keep their signatures. **That file is shared with the `#start-parity` stack, whose rebases will conflict there** (resolve by keeping both sides).

### Test level decisions
| Aspect | Level | Rationale |
|---|---|---|
| Screen text | Unit over `TerminalCapture` fed real escape sequences | Deterministic, no PTY |
| Tool end to end | Integration over the exec-tool handler with a real PTY child | The tool is the contract |

## Decisions & Trade-offs

- `text_grid` is a new accessor this node owns, not a change to `ScreenSnapshot` (`#760`'s), so the
  two stacks never edit one symbol.
- Active buffer via `Vt::view()`, not `Vt::text()` (which returns the primary buffer and scrollback).
- Pass 2 (2026-10-26): the registration and wiring stay out of the contract commit (see `## Draft PR contract`), so the acceptance tests fail for this node's missing implementation and not for a missing list entry.
- Pass 2: the answer's field names follow this changeset (`lines`, `rows`), not the PRD's old example (`rows` for the text, `rows_count`); the PRD is corrected.
- Pass 2: `TextGrid::{rows, cols}` and `GridCursor::{row, col}` are `u16`, as `ScreenSnapshot`'s are.

## TODO

- [x] Record initial discovery (`2026-10-25-home-inbox-terminal-read-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests — reviewed by the orchestrator, developer AFK, 2026-10-26
- [x] TDD Red — write failing unit/integration tests
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

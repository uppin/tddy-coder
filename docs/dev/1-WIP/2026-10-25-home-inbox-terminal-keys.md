# Changeset: Project-home agent types into a session's terminal

**Created:** 2026-10-25
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — four packages, two tools, the control lease used by a non-browser screen with a daemon-held token, a key encoder, the jailed-terminal lease gap
**Stack:** `#home-inbox 5/8` — branch `feature/home-inbox/terminal-keys`, base `feature/home-inbox/desktop-notify`
**PRD:** [PRD-2026-10-25-home-terminal-keys.md](../../ft/daemon/1-WIP/PRD-2026-10-25-home-terminal-keys.md)

## Initial Discovery

[2026-10-25-home-inbox-terminal-keys-initial-discovery.md](./2026-10-25-home-inbox-terminal-keys-initial-discovery.md)

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Record | Verdict |
|---|---|
| `#term-snapshot` #760, #762 | ✅ Met — merged 2026-10-25. Was: ⛔ Blocking (sequencing) — as for `2/8`, via `2/8` |
| [2026-09-11-the-coder-terminal-coordinate-serves-seven-of-nine.md](../todo/2026-09-11-the-coder-terminal-coordinate-serves-seven-of-nine.md) | ⚠ During — the coder participant's lease is a permanent grant; this tool writes only through the daemon's store and lease, never the coder participant |
| `packages/tddy-terminal-rpc/docs/code-issues/complexity-bridge-open-replay-ack-live.md` | ⚠ During — no change to the bridge |
| `packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md` | ⚠ During — encoder and tools in their own modules |

## Affected Packages

- [ ] [`tddy-tool-engine`](../../../packages/tddy-tool-engine/README.md): `ClaimSessionTerminal`, `SendSessionKeys`, `terminal_keys` encoder
- [ ] [`tddy-daemon-rpc`](../../../packages/tddy-daemon-rpc/README.md): the port over the lease + store, the per-home token store
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): public access to the terminal control port
- [ ] [`tddy-tools`](../../../packages/tddy-tools/README.md), [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md): pins + prompt

## Responsibility

- `tddy_tool_engine::terminal_keys::{encode_keys(text: Option<&str>, keys: &[String], bracketed_paste: bool) -> Result<Vec<u8>, String>}` — the named-key table (PRD), `C-a`…`C-z`, `F1`…`F12`; an unknown name is an error before any byte is produced.
- `tddy_tool_engine::project_home_terminal_tools::{CLAIM_SESSION_TERMINAL, SEND_SESSION_KEYS}` + defs; `ProjectHomeTerminalPort::{claim_terminal(&SessionTarget, screen_id) -> Result<(), String>, send_keys(&SessionTarget, screen_id, bytes) -> Result<(), String>}`.
- `tddy_daemon_rpc::exec_tool::home_terminal_leases::HomeTerminalLeases` — `(home session id, target session id) → control_token`, held on `ExecToolRpcHandler`; never returned to the agent.
- Claim = `TerminalControl::claim(target, "project-home:<home id>", steal = true)`; send = `TerminalControl::verify(target, token)` **explicitly** (also for jailed terminals), then `TerminalSession::send_input` on `main`; refusal names the current holder.
- `DaemonSessionHost::terminal_control() -> Arc<dyn TerminalControl>` (public).
- Both tools are **target tools** under `#start-parity 1/9`'s forward and take the optional
  `daemon_instance_id`. The claim and every send run on the host that owns the terminal, so
  `HomeTerminalLeases` lives on that host. It is keyed by the confirmed caller (session id and
  daemon instance) plus the target, so two homes on two hosts never share a token.

## Boundaries

- No release tool; no mouse; no resize; never the home session's own terminal.
- No change to lease semantics, the browser's steal, or `requires_control()` for jailed terminals.
- Reading stays `2/8`'s and needs no lease.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `#home-inbox 2/8` terminal-read | `project_home_terminal_tools` module + `ProjectHomeTerminalPort`, the store accessor, `TextGrid.bracketed_paste`, `ReadSessionTerminal` | adds two tools to that module and two methods to the port; reads `bracketed_paste` before encoding; acceptance tests verify through `ReadSessionTerminal` | change `ReadSessionTerminal`, `TextGrid`, `text_grid`, or the store accessor |
| `#home-inbox 2/8` terminal-read — **green, not only its contract** | `DaemonProjectHome: ProjectHomeTerminalPort`, `ExecToolRpcHandler::from_host` taking `terminal_sessions()`, the registration points `2/8` lists, `TerminalCapture::text_grid` (real), and the `tddy-terminal-rpc` normal dependency of `tddy-daemon-rpc` | green here cannot start before `2/8`'s green. `SendSessionKeys` reads `bracketed_paste` through `read_terminal` when `text` is given; acceptance test 1's last step reads the screen through `ReadSessionTerminal` | add the `tddy-terminal-rpc` dependency or the terminal store to the handler itself |
| `#term-snapshot 1/4` (#760, merged) | the screen model and sniffer | indirectly, through `2/8` | — |
| `#start-parity 1/9` remote-targets (#750, merged) | the generic forward and the caller confirmed at the owning daemon | declares both tools target tools; keys leases by the confirmed caller | change the forward |

## Draft PR contract

The first push publishes `encode_keys`, the two tool constants/defs, the two port methods (with
refusing default bodies), the lease store type and `terminal_control()` (bodies
`// TODO(terminal-keys): implement`, except `terminal_control()`, which is real), and the failing tests.

It deliberately does **not** register the tools and carries **no pins**, as `2/8` does not. The
pin lists also contain `ReadSessionTerminal`, which `2/8` has not registered, and registration needs
`DaemonProjectHome: ProjectHomeTerminalPort`, which is `2/8`'s green. Left to green:

- the tools in `project_home_tool_defs_with` (so the offered catalog), `is_project_home_tool`,
  `is_project_home_target_tool` and `is_project_home_allowed_tool`;
- the `run_locally` and caller-held-elsewhere predicates in `tddy-daemon-rpc`;
- `DaemonProjectHome`'s `claim_terminal` and `send_keys`, and `HomeTerminalLeases` as a field of
  `ExecToolRpcHandler` (built in `from_host` beside the terminal control);
- the pins: `read_only.rs`, `exec_tool_admission.rs`, `project_home_session_tools_acceptance.rs`,
  `project_home_target_host.rs` and `project_home_prompt.md`.

No existing pin test is red on this branch.

**Green must carry the caller's daemon instance through.** `home_caller_confirmed_at_owner`
(`exec_tool/project_home_remote.rs`) returns a `HomeCaller` without the daemon that owns the calling
session, and `HomeTerminalClaimKey` needs it. A caller held here is keyed by this host's own instance.
`DaemonProjectHome` carries it from the two places that build it.

## Green wave

**Wave:** 2 of 4
**Greenable independently:** no — its end-to-end tests read the child's screen through `2/8`'s
`ReadSessionTerminal` (behaviour). The encoder unit tests are independent.
**Concurrent with:** `#home-inbox 3/8` elicit-content, `4/8` desktop-notify
**Blocks:** nothing

    n1 → n3, n4, n7      n2 → n5      n3 → n6, n7      n6 → n8      (cross-stack, replan 2026-10-26: #start-parity #750 [merged] → n2, n5, n6, n7, n8 · #755 → n3 · #751, #758 → n6 · #coagents [merged] → n6)

## State A

Input reaches a PTY through `SendTerminalInput` / bidi `StreamSessionTerminalIO`, gated by the
per-session lease when the terminal `requires_control()` (jailed terminals do not). Leases are
claimed by browser screens, never released, stolen with `steal = true`.

## State B

`ClaimSessionTerminal` takes the lease for `project-home:<home id>` and keeps the token daemon-side;
`SendSessionKeys` verifies that token, encodes text (bracketed paste when 2004 is on) and keys, and
writes them; a browser steal makes the next send fail naming the holder.

## Delta

### New
- `terminal_keys.rs` (tool-engine), `home_terminal_leases.rs` (daemon-rpc)
### Modified
- `project_home_terminal_tools.rs` (+2 tools), the port impl, catalog pins, `project_home_prompt.md`
- `svc_terminal_ports.rs` (+`terminal_control()`)
- `docs/ft/daemon/{project-concept.md,terminal-sessions.md}` at wrap

## Testing Strategy

### Acceptance tests

**`tddy-daemon-rpc`**: `packages/tddy-daemon-rpc/tests/project_home_terminal_keys_acceptance.rs`
- [ ] `after_a_claim_send_session_keys_writes_the_encoded_bytes_and_the_screen_reflects_them`
- [ ] `send_session_keys_without_a_claim_is_refused_and_writes_nothing`
- [ ] `a_browser_steal_after_the_claim_refuses_the_next_send_naming_the_holder_and_a_new_claim_succeeds`
- [ ] `an_unknown_key_name_is_refused_and_writes_nothing`
- [ ] `the_claim_broadcasts_a_control_change_naming_the_project_home_screen`
- [ ] `a_jailed_childs_terminal_follows_the_same_rules`
- [ ] `a_session_on_another_host_is_claimed_and_typed_into_on_that_host` (added in pass 2: the replan's cross-host rule, which the six above never reach)
- [ ] `another_home_that_never_claimed_cannot_type_with_the_first_homes_claim` (added in pass 2: the lease is keyed by caller)

**Failing now** because `ClaimSessionTerminal` and `SendSessionKeys` are not registered: the daemon
answers `PermissionDenied: … a project-home session is read-only`. Test 1's last step reads the child's
screen through `ReadSessionTerminal`, which is blocked on `2/8`'s green (see `## Dependencies`); the
other seven do not read it. The stand-in TUI is a raw-mode shell script that records every byte it is
typed into a file; the jailed child is a registered jail whose stdin channel the test reads.

### Unit tests

All written and failing (pass 2, 6f); they go green with the implementation.

- [ ] `tddy-tool-engine`, `packages/tddy-tool-engine/tests/terminal_keys.rs` (16): every named key's bytes (editing keys, navigation keys, `C-a`…`C-z`, `F1`…`F12`); keys in the order named; text raw and as a bracketed paste; a newline in raw text is `\r`, in a paste verbatim; text before keys; an unknown or near-miss name is refused naming it; one unknown name among known ones refuses the whole request; a request with nothing to type is refused. Fail with "encode_keys: not implemented".
- [ ] `tddy-daemon-rpc`, `packages/tddy-daemon-rpc/tests/home_terminal_leases.rs` (6): a remembered token is given back; none for a claim never made; a new claim replaces the old token; no sharing across caller session, caller daemon instance or target.

### Test level decisions
| Aspect | Level | Rationale |
|---|---|---|
| Encoding | Unit | Pure table |
| Lease + write | Integration over the exec-tool handler with a real PTY and the real lease | The rule is the behaviour |

## Decisions & Trade-offs

Pass 2 (2026-10-28) deviations from the plan, each from reading the code the plan builds on:

1. `claim_terminal` and `send_keys` are on `ProjectHomeTerminalPort` with **refusing default bodies**. Required methods would break `2/8`'s own fake port in `tests/project_home_terminal_tools.rs` and its `DaemonProjectHome` impl. Green removes the defaults.
2. **No registration and no pins** in the first push (see `## Draft PR contract`).
3. `HomeTerminalLeases` is **not yet a field of `ExecToolRpcHandler`**: it would be dead code now and would conflict with `2/8`'s green edits to `from_host`. Wired at green. It is `pub` in `pub mod exec_tool::home_terminal_leases`, with `HomeTerminalClaimKey { caller_daemon_instance_id, caller_session_id, target_session_id }`.
4. The key carries the **caller's daemon instance**, which `home_caller_confirmed_at_owner` does not yet return (see `## Dependencies`).
5. Lease semantics the tests rely on. `TerminalControl::verify` accepts any token while a session has no lease, so a send must check the remembered token first, then verify it, and also that the holder is the home's screen (a deleted session clears its lease). The **first** claim on a session with no holder broadcasts no `ControlChange` (`control_lease.rs`: only a steal does), so the broadcast test starts with a browser holding control.
6. `DaemonSessionHost::terminal_control()` is real code, not a stub: the acceptance tests need it, and `terminal_session_ports` now uses it.

**Refusal wording the acceptance tests pin.** No claim and no holder: contains `ClaimSessionTerminal`. Held by another screen (including a home that never claimed): contains that screen id and `claim` (for example "control is held by `<screen>`; claim it again"). Unknown key: contains the key name.

**Encoding decisions the unit tests pin** (the PRD lists names, not bytes). Normal cursor mode: `Enter` `\r`, `Escape` `0x1b`, `Tab` `\t`, `BTab` `ESC[Z`, `Backspace` `0x7f`, `Delete` `ESC[3~`, arrows `ESC[A..D`, `Home` `ESC[H`, `End` `ESC[F`, `PageUp/Down` `ESC[5~`/`ESC[6~`, `F1`–`F4` `ESC O P..S`, `F5`–`F12` `ESC[15~ 17~ 18~ 19~ 20~ 21~ 23~ 24~`. Key names are case-sensitive. Raw text turns each line break into `\r`; a paste carries the text verbatim. A request with no text and no keys is refused ("nothing to send").

- Claim-then-send with the lease (developer, 2026-10-25): the agent claims; a person reclaims by the
  existing steal; no release tool.
- The token is daemon-held so the agent cannot replay it through another route.

## TODO

- [x] Record initial discovery (`2026-10-25-home-inbox-terminal-keys-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests — reviewed by the orchestrator, developer AFK, 2026-10-28
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

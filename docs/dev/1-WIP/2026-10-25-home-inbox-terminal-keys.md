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
| `#term-snapshot 1/4` (#760, merged) | the screen model and sniffer | indirectly, through `2/8` | — |
| `#start-parity 1/9` remote-targets (#750, merged) | the generic forward and the caller confirmed at the owning daemon | declares both tools target tools; keys leases by the confirmed caller | change the forward |

## Draft PR contract

The first push publishes `encode_keys`, the two tool constants/defs, the port methods, the lease
store type, `terminal_control()` (bodies `// TODO(terminal-keys): implement`), pins, and the failing tests.

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

### Unit tests

- [ ] `terminal_keys`: every named key's bytes; `C-c` = `0x03`; text with newlines as bracketed paste vs raw; empty input; unknown name.

### Test level decisions
| Aspect | Level | Rationale |
|---|---|---|
| Encoding | Unit | Pure table |
| Lease + write | Integration over the exec-tool handler with a real PTY and the real lease | The rule is the behaviour |

## Decisions & Trade-offs

- Claim-then-send with the lease (developer, 2026-10-25): the agent claims; a person reclaims by the
  existing steal; no release tool.
- The token is daemon-held so the agent cannot replay it through another route.

## TODO

- [x] Record initial discovery (`2026-10-25-home-inbox-terminal-keys-initial-discovery.md`)
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

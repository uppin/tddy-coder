# Initial discovery — sandboxed-codebase managed workflow

Changeset: `2026-09-20-sandboxed-codebase-managed-workflow`

## Combined conclusions

1. **The refusal is one `if`, and its stated reason is wrong.**
   `connection_service.rs:1071` refuses `sandboxed_codebase` + `managed_codebase` with
   *"sandboxed_codebase jails the codebase and leaves the agent on the host, managed_codebase jails
   the agent and leaves the codebase on it — a session has one placement, not two."*
   That sentence describes **`sandbox`**, not `managed_codebase`. `managed_codebase` is not a
   placement at all.

2. **`managed_codebase` is orchestration, not confinement.**
   `docs/ft/coder/managed-codebase-workflow.md` § Architecture shows it carrying `recipe` and
   `specialized_agents`, seeding `changeset.yaml`, and setting up a `WorkflowController` plus a
   per-session toolcall listener. The same doc treats sandboxing as an **orthogonal** axis —
   *"**Sandboxed** sessions never mount the repo… **Non-sandboxed** sessions spawn `claude` in a
   host PTY"* — and its start-path line is
   `start_(sandboxed_)claude_cli_session | start_sandboxed_cursor_cli_session (managed_recipe)`,
   i.e. managed already composes with sandboxing on the other placements. The exclusion is
   therefore incidental, not essential.

3. **The picker's absence is a UI nesting bug, independent of the refusal.**
   `CreateSessionPane.tsx:669` renders the agent picker inside `{managedCodebase && (…)}`. The
   component it sits in documents the opposite intent (*"No split guard: an agent is placeable on
   any host… the picker offers the same roster either way"*), and
   `remote-managed-worktree.md` states the picker and the Semantic-index toggle are *"offered and
   submitted on every placement"*. The code contradicts both.

4. **Subagents on this placement are newly possible, and unproven.**
   The roster needs a full RPC client. The HTTP relay never had one
   (`stream.rs`: *"the roster stream has no client for the daemon-HTTP transport; subagent calls
   are refused"*). `SessionToolTransport::DaemonUds` (this branch, commit `d4ef5a69`) is one, and
   `link.rs`/`stream.rs` now group it with `SandboxIpc`. So end-to-end subagent dispatch on a
   sandboxed-codebase session has **never run**.

5. **Precedent exists for removing a placement refusal.**
   `docs/dev/1-WIP/2026-08-31-split-sandbox-orchestration.md` § Responsibility opens with
   *"Remove split+sandbox refusal in `start_split_claude_cli_session`"* — the same move, on the
   same PRD, already made once.

6. **`dangerously_skip_permissions` is a different kind of refusal.** Its reason —
   *"the confinement claim rests on the deny list that flag bypasses"* — is a real coupling, not a
   category error. Lifting it is the developer's decision (taken: lift), and the PRD must state
   what guarantee is surrendered rather than let it go silent.

## State A

| Surface | Today |
|---|---|
| `StartSession` | `sandboxed_codebase` + `managed_codebase` → `invalid_argument`; + `dangerously_skip_permissions` → `invalid_argument` |
| Create-session form | Agent picker + Semantic index render only inside the Managed-codebase block, so they vanish on this placement |
| Roster transport | `DaemonUds` carries roster RPCs (new on this branch); never exercised with a real subagent |
| `specialized_agents` | Sent by the form only when Managed codebase is ticked |

## Exploration 1 — the refusal and its neighbours

- `grep -rn "sandboxed_codebase" packages/ --include="*.rs" | grep -iE "managed|exclusive|refus"`
  → all six refusals pinned in `packages/tddy-session-lifecycle/tests/sandboxed_codebase_placement_acceptance.rs`
  (lines 355, 374, 395, 415, 434, 453, 475).
- `sed -n '1060,1085p' packages/tddy-session-lifecycle/src/connection_service.rs` → the
  `requested_codebase_id`, `managed_codebase` and `sandbox` arms, in that order.
- `packages/tddy-session-lifecycle/src/connection_service.rs:1135` → `if !managed_codebase` is the
  only other site keying on the flag in this file.

## Exploration 2 — what managed_codebase actually provides

- `docs/ft/coder/managed-codebase-workflow.md` lines 30–60 — transition relay, `TDDY_SOCKET`
  per-session listener, the architecture block quoted above.
- `docs/ft/coder/managed-codebase-subagents.md` — the `subagent_*` tool surface.
- `packages/tddy-web/src/components/sessions/sessionRosterHalf.ts` — the roster lives on the
  **codebase half**; `rosterHalfOf` maps a session to `codebaseSessionId`.

## Exploration 3 — the form

- `CreateSessionPane.tsx:669` `{managedCodebase && (<CreateSessionManagedCodebaseFields … />)}`
- `CreateSessionManagedCodebaseFields.tsx:113` `{agentPickerSection}`, with the "no split guard"
  comment at 110–112 and the Semantic-index toggle immediately after.
- `CreateSessionPane.tsx:274` `currentPlacement` is a single choice
  (`sandboxedCodebase` | … ), and `:307` `setSandboxedCodebase(next === "sandboxedCodebase")` —
  the placement radio is what makes the two look mutually exclusive in the UI.

## Exploration 4 — deferred-work cross-check

Scanned `packages/*/docs/code-issues/` and `docs/dev/todo/`.

| Record | Verdict |
|---|---|
| `packages/tddy-session-lifecycle/docs/code-issues/complexity-svc-spawn-split-agent-spawn-split-agent.md` | ⚠ during — 233 lines / nesting 5 / 9 params, already 3.9× budget, **unclaimed**; PR #518 grew it and this change touches the same function |
| `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-connection-service.md` | ⚠ during — ~1,930 lines, **unclaimed**; the refusal being deleted lives here, so this change *shrinks* it |
| `packages/tddy-session-lifecycle/docs/code-issues/complexity-svc-start-sandboxed-claude-cli-session-start-sandboxed-claude-cli-session.md` | ⚠ during — adjacent start path |
| [`docs/dev/todo/2026-08-17-session-agent-roster…md`](../todo/2026-08-17-session-agent-roster-the-sandbox-bridge-and-other-deliberate-gaps.md) | ℹ answered — the in-jail subagent blocker closed 2026-08-29; our inverted placement needs the **`DaemonUds`** path instead, landed on this branch |
| [`docs/dev/todo/2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md`](../todo/2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md) | — unrelated; this placement is `claude-cli` only |

No 🚧 claimed issue sits in this change's path.

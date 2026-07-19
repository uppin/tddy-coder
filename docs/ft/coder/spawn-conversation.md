# Spawn Conversation from a Workflow

## Overview

A managed workflow agent can spawn a **new interactive Claude CLI conversation on a freshly
created git worktree** by calling the `spawn_conversation` MCP tool. The spawned conversation is a
full child session (its own worktree, its own `claude` process, its own LiveKit room) tagged with
`orchestrator_session_id = <parent session id>`. The web renders each such child as a **tab inside
the parent session**, next to the reserved "Agent" tab and any bash tabs.

The first consumer is the **grill-me** workflow: after its **Create plan** phase writes the plan
brief, the agent calls `spawn_conversation` seeded with the brief, handing the plan to a fresh agent
that starts on a new branch/worktree — surfaced to the user as a new tab in the grill-me session.

## Technical Context

### Design Rationale

The daemon already materializes "a new worktree + a new `claude` conversation tagged with a parent"
for the **PR-stack orchestrator** via `spawn-child` (`ChildSpawnHandler` →
`spawn_claude_cli_session_inner(stack_parent = Some(...))`). That mechanism is PR-stack-specific: it
resolves a *planned-PR node* from the orchestrator's `changeset.stack`, which a grill-me session does
not have. Rather than widen the PR-stack path (and risk its live behavior), we add a **parallel,
additive** capability: a generic `spawn_conversation` tool + `ConversationSpawnHandler` that takes an
explicit **prompt** (and optional **branch**/**base_ref**) instead of a node id, reusing the same
`spawn_claude_cli_session_inner` spawn path and the same `orchestrator_session_id` discovery contract.

Because a spawned conversation is a real child session, the web needs no new RPC to discover it: the
existing `ListSessions` poll already returns `SessionEntry.orchestrator_session_id`. The parent
session's runtime derives its child tabs by filtering the session list on
`orchestratorSessionId === parentSessionId`.

### Target Consumers

- **grill-me recipe (`tddy-workflow-recipes`)** — its **Create plan** prompt instructs the agent to
  call `spawn_conversation` after writing the brief.
- **tddy-daemon** — binds a `GrillMeConversationSpawnHandler` on a grill-me managed session's
  per-session toolcall listener; the handler calls `spawn_claude_cli_session_inner`.
- **tddy-web `SessionRuntime`** — renders discovered child sessions as tabs and attaches the child
  session's runtime when its tab is selected.

### Success Metrics

- **Continuity**: a completed grill-me plan can be handed to a fresh implementation agent without the
  user manually starting a new session.
- **No regression**: the PR-stack `spawn-child` path (tool, wire type, handler, verb) is untouched;
  its tests keep passing.
- **Discovery**: a spawned child appears as a tab in the parent within one `ListSessions` refresh.

## API/Library Requirements

### Core Capabilities

- **MCP tool `spawn_conversation`** (`tddy-tools`): input `{ prompt: string, branch?: string,
  base_ref?: string }`. Relays `{"type":"spawn-conversation", prompt, branch, base_ref}` over
  `TDDY_SOCKET` and returns the daemon response (`{"status":"ok","session_id":...}` on success).
  Errors when `TDDY_SOCKET` is unset (not a managed session).
- **Toolcall verb `SpawnConversation`** (`tddy-core`): `SpawnConversationRequestWire { prompt,
  branch, base_ref }`; dispatched to a per-session `ConversationSpawnHandler`. Reuses the existing
  `ToolCallResponse::SpawnChildOk { session_id }` for the success wire shape. When no handler is
  bound, the verb is rejected with an actionable message (never a silent no-op).
- **`GrillMeConversationSpawnHandler`** (`tddy-daemon`): resolves the parent session's model +
  project, derives a branch (from `branch` or a slug of the prompt), and calls
  `spawn_claude_cli_session_inner(branch_worktree_intent = "new_branch_from_base",
  stack_parent = Some(parent_session_id), managed_recipe = None)` — a plain interactive claude-cli
  child on a new worktree. Returns the new child `session_id`.
- **Child-as-tab rendering** (`tddy-web`): the parent `SessionRuntime` derives
  `childSessions = sessions.filter(s => s.orchestratorSessionId === parentSessionId)`, renders a tab
  per child, and attaches the child session's runtime when its tab is active.

### Developer Experience (DX) Requirements

- The new capability threads through the daemon spawn plumbing exactly like the existing
  `child_spawn_handler` parameter — an additive optional `conversation_spawn_handler` at each hop.
- grill-me needs no new session type or launch path: a grill-me session started with
  `managed_codebase = true` already gets a per-session toolcall listener with `TDDY_SOCKET` exported;
  the only new wiring is binding the conversation handler for the `grill-me` recipe.

## Technical Requirements

### API Contract

- **Wire (agent → daemon over `TDDY_SOCKET`)**: request `{"type":"spawn-conversation","prompt":<str>,
  "branch":<str|null>,"base_ref":<str|null>}`; success response `{"status":"ok","session_id":<str>}`;
  failure `{"status":"error","message":<str>}`.
- **Handler contract**: `ConversationSpawnHandler::spawn_conversation(prompt, branch, base_ref) ->
  Result<session_id, message>`. `Ok` maps to `SpawnChildOk`, `Err(message)` maps to `Error{message}`,
  mirroring `handle_spawn_child`.
- **Discovery contract**: the spawned child session's changeset records
  `orchestrator_session_id = <parent>`, surfaced via `SessionEntry.orchestrator_session_id`
  (`connection.proto`). No new RPC.

### Child session shape

- **Branch/worktree**: `new_branch_from_base`; the child chains onto the parent (grill-me) branch via
  the existing `resolve_chain_base_ref` (the parent is a real code session with a branch).
- **Recipe**: `None` — a plain interactive `claude` conversation. The user drives it; no managed
  graph auto-runs in the child.
- **Seed prompt**: references the plan. The grill-me handler passes the agent's `prompt` verbatim; the
  grill-me **Create plan** prompt is responsible for composing a prompt that points at the plan
  (absolute session-artifact brief path + the committed `plans/<slug>.md`).

### Architecture

- **`tddy-core`** `toolcall/`: `SpawnConversationRequestWire` (`mod.rs`), `ConversationSpawnHandler`
  trait + `with_conversation_spawn_handler` builder + `"SpawnConversation"` dispatch +
  `handle_spawn_conversation` (`listener.rs`), reusing `ToolCallResponse::SpawnChildOk`.
- **`tddy-tools`** `server.rs`: `SpawnConversationInput` + `spawn_conversation` tool; the request-JSON
  builder is a pure fn for unit testing.
- **`tddy-daemon`**: `GrillMeConversationSpawnHandler` in `connection_service.rs`; a
  `conversation_spawn_handler` parameter threaded through `spawn_claude_cli_session_inner` →
  `prepare_managed_workflow_inner` → `set_up_managed_workflow`/`resume_managed_workflow`/
  `build_managed_workflow` → `start_session_toolcall_listener` (bound via
  `.with_conversation_spawn_handler`). A `recipe_spawn_handlers(recipe, …)` helper selects the
  `(child, conversation)` handler pair per recipe (`grill-me` → conversation handler; `pr-stack` →
  child handler).
- **`tddy-workflow-recipes`** `grill_me/prompt.rs`: **Create plan** prompt gains a required final step
  to call `spawn_conversation` after the brief files exist (and to commit `plans/<slug>.md` first).
- **`tddy-web`** `components/sessions/`: `useChildSessions` derives child tabs from the session list;
  `SessionTerminalTabs` renders them; `SessionRuntime` mounts a selected child's runtime pane.

## Acceptance Criteria

- [ ] The `spawn_conversation` MCP tool relays a `spawn-conversation` request over `TDDY_SOCKET` and
      errors when `TDDY_SOCKET` is unset.
- [ ] A bound `ConversationSpawnHandler` turns a `SpawnConversation` verb into
      `{"status":"ok","session_id":...}`; with no handler bound the verb is rejected with a message.
- [ ] A grill-me managed session binds a `ConversationSpawnHandler`; a non-grill-me recipe (e.g. `tdd`)
      does not.
- [ ] A spawned conversation is a child session whose changeset records
      `orchestrator_session_id == <parent session id>`, on a new branch/worktree.
- [ ] The grill-me **Create plan** prompt instructs the agent to call `spawn_conversation` after
      writing the brief.
- [ ] The web renders a discovered child session as a tab in the parent session runtime; selecting it
      attaches the child session and shows its pane. A grill-me session with no children shows only
      the Agent tab.

## Testing Strategy

- **Unit (`tddy-core`)**: `SpawnConversationRequestWire` deserialization (prompt required;
  branch/base_ref default `None`); `ToolcallRpcService` dispatch of `SpawnConversation` through a
  bound fake handler; rejection when no handler is bound.
- **Unit (`tddy-tools`)**: `spawn_conversation` errors without `TDDY_SOCKET`; the relayed request JSON
  has the `spawn-conversation` shape.
- **Unit/integration (`tddy-daemon`)**: `set_up_managed_workflow` binds the conversation handler; the
  `recipe_spawn_handlers` helper returns a conversation handler for `grill-me` and none for `tdd`; a
  spawned child's changeset has `orchestrator_session_id == parent`.
- **Unit (`tddy-workflow-recipes`)**: the **Create plan** prompt names `spawn_conversation`.
- **Acceptance (Cypress component, tddy-web)**: a child `SessionEntry` with `orchestratorSessionId ===
  parentId` renders as a tab; selecting it attaches the child session; a childless session shows only
  the Agent tab.

## Related Documentation

- [Session Participant RPC & Metadata](session-participant-rpc.md) — session-scoped RPC + terminals.
- [PR Stacking](pr-stacking.md) — the `spawn-child` precedent this generalizes.
- [Session Terminal Tabs](../web/session-terminal-tabs.md) — the tab bar this extends.
- [Workflow Recipes](workflow-recipes.md) — grill-me and the managed-workflow model.

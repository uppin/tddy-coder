# Spawn child Claude CLI session (Hello World bash smoke test)

## Problem

The operator wants to validate **managed child session spawning** in tddy: from this parent **grill-me** session, start a **new** interactive **Claude CLI** conversation on a **fresh worktree**, seeded with the task **"build a Hello world app in bash"**.

This is an **operator smoke test**, not a request to add or change product code for spawning itself. The value is confirming that **`spawn_conversation`** (via **`tddy-tools spawn-conversation`**) creates a visible child session with the expected recipe, worktree defaults, and orchestrator linkage.

**Success for this effort (parent scope):** a new managed child session appears and is running (TUI and/or web). The child is not required to finish the Hello World app for the parent smoke test to pass, though the child prompt still carries that implementation task.

## Q&A

| Question | User decision |
|----------|----------------|
| What should this grill-me effort produce? | **Run spawn now** — use spawn in the current session as a smoke test / operator action, not new product feature work. |
| Child session recipe? | **`free-prompting`**. |
| What counts as success? | **Child session starts** — new managed session visible with Claude CLI running; no requirement that Hello World code exists before parent sign-off. |
| When should the parent run spawn? | **After Create plan** — brief written first, then parent invokes spawn in the same session. |
| Child worktree / branch? | **All defaults** — derive branch from prompt; use session base ref; no custom branch name. |

**Original request:** spawn new conversation with Claude CLI with task *"build a Hello world app in bash"*.

## Analysis

- **Scope boundary:** Parent work ends at reliable spawn + session visibility. Hello World implementation belongs in the **child** worktree unless the operator expands scope later.
- **Tooling:** Handoff uses **`tddy-tools spawn-conversation`** with **`TDDY_SOCKET`** set (managed session). MCP **`spawn_conversation`** is equivalent when exposed to the agent; CLI is the documented grill-me handoff path.
- **Recipe:** Child should be **`free-prompting`** so the graph stays minimal and the agent focuses on the bash task.
- **Worktree:** Default derivation avoids colliding with the parent feature branch **`feat-workflow-adding-new-conversation`** and keeps the smoke test reproducible.
- **Risks:** Spawn fails if socket/session env is missing; daemon must accept new sessions; Claude CLI backend must be allowed for the project. Child may idle if prompt is unclear — brief paths are included in the seed prompt.
- **Dependencies:** Running **tddy-daemon**, parent session on worktree **`feat-workflow-adding-new-conversation`**, compiled **`tddy-tools`** with **`spawn-conversation`** subcommand.
- **Open questions:** None blocking spawn; optional follow-up: confirm child appears in web UI and TUI session list with correct orchestrator parent id.

## Preliminary implementation plan

### Phase 1 — Parent (this session)

1. Persist this brief at the session artifact path and under **`plans/spawn-child-hello-bash-smoke-grill-me-brief.md`** in the repo working copy; commit the plans file.
2. Run **`tddy-tools spawn-conversation`** with a prompt that:
   - Points the child at both brief paths.
   - States the child task: **build a Hello world app in bash** in the child worktree.
   - Uses **default** `branch` / `base_ref` (omit branch in JSON).
3. Verify parent smoke test: note returned **child session id**; confirm child session is listed and Claude CLI is active.

### Phase 2 — Child (spawned session)

1. Confirm worktree layout and recipe (**`free-prompting`**).
2. Add a minimal bash script (e.g. **`hello.sh`**) that prints **`Hello, World`** (or agreed greeting).
3. Make executable (`chmod +x`) and run once to verify output.
4. Stop when runnable; no requirement to open a PR unless the operator asks.

### Phase 3 — Optional validation

- Parent operator checks orchestrator → child relationship in session metadata.
- Document child session id and branch name in a scratch note if debugging spawn regressions.

**Post-plan command (parent):**

```bash
tddy-tools spawn-conversation --data '{"prompt":"Read the plan brief at /var/tddy/Code/tddy-coder/.worktrees/feat-workflow-adding-new-conversation/tmp/.tddy/sessions/019f7dd3-b07a-7821-b56c-ac1f8f9f8429/artifacts/grill-me-brief.md and plans/spawn-child-hello-bash-smoke-grill-me-brief.md. This is a free-prompting child session: build a Hello world app in bash in this worktree (minimal runnable script)."}'
```

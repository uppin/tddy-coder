# Session Branch Conflict — prompt instead of silently suffixing the branch

Creating a session with an explicit new-branch name whose branch is **already owned by another
session** no longer silently creates `<branch>-1`. The daemon refuses to create anything and
reports the conflict; the requesting surface asks the operator to choose between switching to the
owning session, adding a second agent to it, or naming a different branch.

Silent suffixing was invisible: `StartSessionResponse` carried no branch, so an operator who asked
for `feat/auth` and got `feat/auth-1` learned about it only by reading the session's worktree path.

## Scope of the conflict

A conflict is **only** a branch that an existing session owns — a session whose
`Changeset.branch` equals the requested name. A branch that exists in git with no session behind it
(a stale local branch, a branch created by hand) is **not** a conflict and keeps the existing
suffixing behaviour, because there is no session to switch to and no second agent to add.

Ownership resolution matches `QueryBranch`: scan the caller's sessions root, prefer an **active**
session, tie-break on the most recent `updated_at`.

## API Surface

### `StartSessionRequest` (proto field 30)

```proto
// How to handle `new_branch_name` naming a branch another session already owns. Only consulted in
// "new_branch_from_base" mode with a non-empty `new_branch_name`.
//   ""       — create a suffixed branch (`<name>-1`, `-2`, …). The behaviour every existing caller gets.
//   "reject" — create nothing; return `StartSessionResponse.branch_conflict` instead.
string on_branch_conflict = 30;
```

Opt-in by design: only a surface that can actually prompt an operator asks to be rejected.
Non-interactive callers — workflow recipe hooks, PR-stack chain spawns, `RestoreSessionWorktree` —
have no one to ask and keep suffixing.

### `StartSessionResponse` (proto field 5)

```proto
message StartSessionResponse {
  string session_id = 1;
  string livekit_room = 2;
  string livekit_url = 3;
  string livekit_server_identity = 4;
  // Set only when the request asked for "reject" and the branch is owned. `session_id` is then
  // empty: no session directory, no branch and no worktree were created.
  BranchConflict branch_conflict = 5;
}

message BranchConflict {
  // The requested branch that is already owned.
  string branch = 1;
  // The session that owns it. `exists` is always true here.
  BranchSession owner = 2;
  // The first free suffixed name (e.g. "feat/auth-1") — what the "" mode would have created.
  // Pre-fills the rename field in the operator's prompt.
  string suggested_branch_name = 3;
}
```

`BranchSession` is the existing `QueryBranch` message (`session_id`, `is_active`, `status`).

A conflict is reported as a **populated response field, not an RPC error.** `tddy_rpc::Status`
carries only a code and a message — it has no details field, and `status_to_error_body` hardcodes
`details: vec![]` — so an error could not carry the owning session id or the suggested name without
plumbing details through three conversion layers, and `StartSession` is additionally forwarded
between hosts over LiveKit, where only the response message is guaranteed to survive intact.

## Behavior

- The guard runs in `start_session` **before** the session-type dispatch, so it covers `tool`,
  `claude-cli`, `cursor-cli` and `workspace` sessions from one place.
- It runs only for `branch_worktree_intent = "new_branch_from_base"` with a non-empty
  `new_branch_name`. Generated names (`claude-cli/<short-id>`, `workspace/<short-id>`) are derived
  from the session uuid and cannot collide.
- `work_on_selected_branch` is never rejected — it is the intent that *deliberately* joins an
  existing branch, and is what the "add another agent" choice re-submits.
- A rejected request creates nothing: no session directory, no `changeset.yaml`, no branch, no
  worktree, and no remote push.
- Peer-agent creation (`repo_path` set, branch fields empty) never conflicts — it creates no branch.

## Operator prompt

The three choices, and the request each one produces:

| Choice | Action |
|---|---|
| **Switch to the existing session** | No RPC. Select and attach the owning `session_id`. |
| **Add another agent on this branch** | Re-submit with `branch_worktree_intent = "work_on_selected_branch"` and `selected_branch_to_work_on = branch`. The new session shares the owning session's worktree — two agents on one checkout. |
| **Use a different branch name** | Re-submit `new_branch_from_base` with the operator-typed name, pre-filled with `suggested_branch_name`. |

A re-submitted name that is *also* owned conflicts again and re-opens the prompt.

"Switch to the existing session" is always offered: by definition a conflict means an owning
session exists.

### tddy-web

`CreateSessionPane` sends `on_branch_conflict = "reject"` for every non-peer creation. A response
carrying `branch_conflict` opens `BranchConflictDialog` instead of navigating; the form stays
mounted behind it with its values intact, so cancelling the dialog returns the operator to the
filled form.

The dialog names the owning session and whether it is active, e.g. *"`feat/auth` is already used by
session `a1b2c3d4` (active)."*

The rename choice is an editable input pre-filled with `suggested_branch_name`; submitting it
re-runs creation with the typed value.

Dialog test ids: `branch-conflict-dialog`, `branch-conflict-owner`, `branch-conflict-switch-btn`,
`branch-conflict-add-agent-btn`, `branch-conflict-rename-input`, `branch-conflict-rename-btn`,
`branch-conflict-cancel-btn`.

See [Session Drawer § Create Session](../web/session-drawer.md#create-session).

### Telegram

Telegram spawns sessions without going through `StartSession` — `/start-claude` picks a project,
intent, base branch and model, then calls `setup_worktree_for_session_with_optional_chain_base`
directly — so it needs the same ownership check at its own call site, sharing the daemon's detection
helper.

The branch name is derived, never typed: `feature/<slug(changeset.name)>` for claude-cli sessions,
so two sessions named the same collide. Cursor-cli sessions derive `cursor-cli/<short-id>` from the
session id and cannot collide.

The check runs when the base-branch choice resolves `new_branch_name`
(`handle_telegram_branch_callback`), not at spawn time, so the operator is asked before the model
picker rather than after committing to a model. When the branch is owned, an inline keyboard offers:

| Button | Action |
|---|---|
| `Switch to <label>` | Enters the owning session (binds the chat to it, same as tapping its Enter button). The half-configured pending session is left un-spawned, exactly as abandoning any picker mid-flow does today. |
| `New agent on <branch>` | Rewrites the changeset to `work_on_selected_branch` on that branch, clears `new_branch_name`, and continues to the model picker. |
| `Use <suggested>` | Keeps `new_branch_from_base` with `suggested_branch_name` and continues to the model picker. |

Telegram gets a one-tap suffixed name rather than the web's editable field: `callback_data` is capped
at 64 bytes, so a branch name cannot be carried in a callback, and free-text branch entry would need
a pending-input state machine the bot does not have. The name is therefore computed server-side and
re-derived from the changeset when the callback arrives.

New callback prefix: `CB_TELEGRAM_BRANCH_CONFLICT = "tbc:"`, payload
`tbc:<choice>:<proj_idx>:<session_id>` where `<choice>` is `sw` | `na` | `sg`.

### Surfaces that keep suffixing

`tddy-coder`'s TUI and plain-CLI paths bootstrap a worktree *inside an already-running session* via
recipe hooks, not as part of creating one — "switch to the existing session" and "add a second agent"
are not available choices there — so they keep the suffixing behaviour. The same applies to the other
non-interactive callers: PR-stack chain spawns, `RestoreSessionWorktree`, and workflow recipe hooks.

`tddy-sandbox-app` and `tddy-tools remote start-session` send no `new_branch_name` at all, so the
daemon generates `claude-cli/<short-id>` / `workspace/<short-id>` and they cannot collide.

## Interaction with existing behaviour

- `create_worktree_with_retry` (`packages/tddy-core/src/worktree.rs`) is unchanged and still the
  path for `on_branch_conflict = ""`, including the case where the colliding branch or worktree
  directory exists with no session behind it.
- `suggested_branch_name` is computed by the same rule the retry loop uses (first free
  `<branch>-<n>`, `n` from 1), read-only — it inspects git without creating anything.

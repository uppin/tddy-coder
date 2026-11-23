# PR stacking

**Product area:** Coder  
**Updated:** 2026-11-22

## Summary

PR stacking support adds a **single orchestrating session** — with its own worktree and branch (usually the final PR in the stack) — that plans a stack of child PR sessions and then **operates that stack interactively** with the developer. When concurrent PRs exist the stack is a **DAG (directed acyclic graph)** rather than a simple chain.

A stack is a **project record**, not a workflow: it is created, planned and driven through the web's [project stack view](#opening-a-project-stack-by-id) and through the [PR-management tools](#pr-management-tools) an orchestrator-role agent holds. No workflow recipe plans or drives a stack. The operator prompts the orchestrator agent, which manages the stack explicitly through those tools; there is no autonomous `assess → spawn / merge / repoint` loop. Each planned PR carries an **internal status** — a computed "does this node need action?" signal (e.g. `needs-repoint`, `has-conflicts`, `ready-to-merge`) that is auto-derived from git + GitHub reality but can be overridden/annotated by the agent (see [Internal PR status](#internal-pr-status)).

A session recorded with the retired `pr-stack` workflow recipe (or its aliases `plan-pr-stack` / `orchestrate-pr-stack`) still lists and opens — in the workflow chat view — but its resume is refused with a message naming the recipe. Its stack, already migrated into its project's store, is reached through the project stack view like any other.

The design extends the existing single-level session chaining mechanism (see [Git integration base ref — Session chaining](git-integration-base-ref.md)) to a full ordered graph, closes the Telegram-only gap by exposing transport-agnostic spawn helpers and CLI flags, and adds a uniform progress-tracking contract that all child sessions satisfy via host-level hooks rather than agent promises.

## Vocabulary — work items and work queues

A planned PR is a **work item** and the container that holds them is a **work queue**. Every surface
uses these names: the `work_queue.WorkQueueService` RPCs (`AddWorkItem`, `RepointWorkItem`,
`ReorderWorkItem`, `ResolveWorkItemBase`, `LinkWorkItem`, `ListProjectWorkQueues`, `GetWorkQueue`,
`CreateWorkQueue`; `GetPrStatus`, `QueryBranch` and `PullBaseIntoBranch` describe PRs and branches and
keep their names), the agent tools (`work_queue_*`, `work_item_*`, `work_pr_read` / `work_pr_search` /
`work_pr_comments`), the `tddy-work-queue` crate, the `tddy-workqueue-web` package and the
`/sessions/project/:projectId/queue/:queueId` route. Old tool and RPC names are unknown: no alias is
kept. Data written before the rename loads unchanged — the store directory (`stacks/`) and the YAML keys
(`stack_id`, `stack`, `node_id`) keep their spelling on disk.

## Session YAML enrichment

`Changeset` in `tddy-core` carries one optional stack field, additive (`Option` + `serde default`) and forward/backward safe because `Changeset` carries no `deny_unknown_fields` guard. A session's work queue is not in its changeset: it is a file of its own in the session directory (see [Where a stack lives](#where-a-stack-lives)).

- **`orchestrator_session_id: Option<String>`** — present only on **child** sessions. A back-reference from child to its orchestrating session. This is distinct from `SessionMetadata.previous_session_id`, which points at the base-branch source (in a DAG that may be a sibling node, not the orchestrator). Having two separate links allows a child's git history to be built on a sibling while its orchestrator is still traceable.

## Stack data model

An orchestrator's stack is a `Stack` value, held by its project (see [Where a stack lives](#where-a-stack-lives)):

```
Stack { version: u32, nodes: Vec<WorkItem> }
```

Each node in the DAG is a `WorkItem`:

| Field | Description |
|-------|-------------|
| `node_id` | Stable planner identifier (e.g. `"n1"`). Exists before a child session is materialized. |
| `title` | Human-readable PR title. |
| `description` | Longer description for the PR body. |
| `branch_suggestion` | Planner's suggested branch name — a *planned* name, set at planning time. It names no ref and never satisfies the spawn gate; it is the name the child worktree is asked to create. |
| `branch` | Actual branch name once the child worktree has created it — "a branch that exists". Planning leaves this `None`. **This is the spawn gate and the durable link key.** |
| `session_id` | Child session id once the node is materialised. A *fallback* route to resolving `branch` only; the stack does not depend on it, and a closed or cleaned-up session never blocks the nodes below. A deleted session leaves the id in place — the node is then **orphaned** and offers "Start session" again (see [PR-Stack live status § Orphaned-node recovery](pr-stack-live-status.md#orphaned-node-recovery-added-2026-07-26)). |
| `parents` | List of parent `node_id` values. Empty list = root node (branches off the stack base). More than one entry = DAG node that integrates multiple unmerged parents. |
| `pr_status` | Mirrors `GithubPrStatus` (`phase` one of `planned`, `open`, `merged`, `closed`, `error`). Reflects **GitHub reality**. |
| `child_state` | Coarse mirror of the child session's `WorkflowState`. |
| `internal_status` | Optional `PrInternalStatus` — the **action-needed** signal, orthogonal to `pr_status`. See [Internal PR status](#internal-pr-status). |

**Derived, never persisted:** effective base refs are computed on demand, not stored. The predicate `WorkItem::is_skipped()` returns true when `pr_status.phase == "merged"`. Base-ref derivation climbs the `parents` list, skipping merged ancestors, and returns the nearest non-skipped ancestor **branches** as `origin/<branch>` refs; when all ancestors are merged the node's effective base collapses to the stack bottom (i.e. `origin/main` or equivalent). A non-merged ancestor that owns no branch contributes nothing — it is never given a synthesized `origin/<node_id>` ref, because nothing ever created that ref.

**Helpers on `Stack`:** `topo_order` (Kahn sort; cycle → error), `effective_base_refs(node_id, stack_bottom_base) -> Vec<String>`, `base_ref_for_spawn(node_id, stack_bottom_base) -> Result<String, WorkflowError>` (the spawn base; refuses when a non-merged parent owns no branch), `node(node_id)`.

### Where a stack lives

A work queue **belongs to a session**: one queue per owning session, and **the queue's id is that session's id**. Any session can own one, whatever its role. Each host keeps the queues of its sessions in their session directories:

```
<tddy_data_dir>/sessions/<session_id>/work-queue.yaml
```

so a queue lives and is removed with its session, and is read and written on the daemon that holds the session.

Each file is a `SessionWorkQueueRecord { owner_session_id, project_id, created_at_unix_ms, title?, queue }`. `tddy_changeset::session_work_queue::SessionWorkQueues` (also `tddy_core::session_work_queue`) addresses queues by session id: `create_for_session(session_id, project_id, created_at_unix_ms)`, `read(session_id)` (`None` for a session that owns none), `update(session_id, f)` and `list_for_project(project_id)`. A session id that does not name exactly one path segment is refused (`InvalidSessionId`), never joined onto the path; creating a second queue for one session is `AlreadyExists`; updating a session that owns none is `NoQueue`, whose message carries the hint *"a queue is created when a session is started with a work queue"* (`NO_WORK_QUEUE_HINT`); an unparseable record is `Malformed`.

`tddy_changeset::work_queue_store::WorkQueueStore` is the **project-scoped view** the RPC handlers, the agent tools and the session listing use: `read(project_id, queue_id)`, `update(project_id, queue_id, f)` and `list(project_id)` (sorted by queue id), over the same files. It refuses a queue that belongs to another project (`NotFound`) and a project id that is not one path segment (`InvalidProjectId`), and it creates nothing. It is constructed over the sessions base (the directory holding `sessions/` and `projects/`).

Every write is a read-modify-write of one record under a **per-record lock** — an in-process mutex keyed by the record's path, shared by every handle in the process — finished by an atomic rename, so two concurrent writers to one queue both land and a reader never sees a half-written record. The lock is in-process only: the daemon is the one process that writes a host's queues.

**A queue comes into being at session start, and only then.** A session started **"with work queue"** (`StartSessionRequest.with_work_queue`, the `StartSession` tool's `with_work_queue` argument, the New-session form's checkbox) owns an empty queue from its first moment. An **orchestrator-role** session (the project's main session and every other orchestrator) gets one without asking — `starts_with_work_queue(with_work_queue, session_role)` is the one place a role is consulted. An orchestrator that has no queue yet (one that predates work queues) receives its empty queue when it is next resumed. A session started without the option owns none: no write ever creates a queue, so a write to it is refused with the hint above.

The session directory also keeps everything that is **about a run** rather than the plan: the stack-op journal, per-node documents, `stack-plan.yaml` and `stack-status.*`.

**Project stack records are not read.** The `<tddy_data_dir>/projects/<project_id>/stacks/` directories that held stacks as project records are deleted at every daemon start, before the session host serves (`drop_old_project_stack_records`, run from the daemon's last-run sweep; idempotent, logged). Their stacks are not migrated.

### Effective spawn branch *(added 2026-07-26)*

Both the node link and chain-base resolution used to key on the spawn request's `new_branch_name`, which
is **empty** when the operator resumes an existing branch (`work_on_selected_branch`). A resumed spawn
therefore never re-linked to its node — `work_item_for_spawn` returns `None` for a blank branch — so
each click produced another unlinked session and the row stayed stuck in its recovered state.

The linking key is now the spawn's **effective branch** (`connection_service::effective_spawn_branch`):

- `new_branch_name` (trimmed) when a branch is being created;
- otherwise `selected_branch_to_work_on`, normalised through `tddy_core::worktree::local_branch_name`
  so an `origin/`-prefixed selection resolves to the local branch name;
- a new-branch spawn ignores `selected_branch_to_work_on` entirely — the dialog sends that field
  unconditionally, so "prefer whichever field is non-empty" would pick the wrong one.

Applied at the **node-link** sites of the CLI spawn paths, which every CLI agent takes and which all
take a `work_queue_parent` (`opencode_cli_spawn::start_cli_harness_session` on the host,
`svc_start_sandboxed_cli_harness_session` in a jail, both through
`cli_session_worktree::provision_cli_worktree`), so **a resumed spawn re-links its node** and the
recovery sticks. `resolve_chain_base_ref` deliberately keeps keying on `new_branch_name`: under
`work_on_selected_branch` a resolved chain base would make worktree setup run a real
`fetch_chain_pr_integration_base` — a fetch that can fail, and that is pointless for a resume which
creates no branch.

**Branch resolution helpers** (free functions in `changeset.rs`): `resolve_stack_node_branch(sessions_root, node) -> Option<String>` — the node's own `branch`, else the `branch` in its child session's changeset (a missing session directory resolves to `None`, never an error) — and `read_stack_with_resolved_branches(sessions_root, orchestrator_session_id)`, the orchestrator's stack with every node's `branch` hydrated through that resolver. The hydrated copy is read-only; persisting it would write a fallback-derived branch onto a node that never recorded one.

**Read/write helpers** (all write by temp-then-rename):

- `read_session_stack(session_dir)` — the work queue the session owns, read from its own directory; `None` for a session that owns none.
- `update_stack_atomic(session_dir, f)` — apply a mutation closure to the queue the session owns, under that record's lock. A session that owns no queue is refused with the hint; no write creates one.
- `link_stack_node_to_child_session(orchestrator_dir, node_id, child_session_id, branch)` — record session id and branch on the node after spawning.
- `sync_stack_node_from_child(orchestrator_dir, sessions_root, node_id)` — read the child's `changeset.yaml` and propagate `state.current` → `child_state` and `workflow.github_pr_status` → `pr_status`.

Two writers append a node — `add_work_item` and `adopt_pr_as_stack_node` — and both go through two private helpers, `read_stack(dir, op)` (the changeset's stack, or an empty one) and `append_node_atomic(dir, node, op)` (the `update_stack_atomic` write, with `assign_missing_display_order` as the closure's first statement and `next_display_order` as the node's position, both decided inside the closure). Ordering is a stack invariant rather than each writer's business: a third writer that spelled the closure out and forgot `assign_missing_display_order` would hand its node a position an older node already holds. Each writer still spells its `WorkItem` literal out in full, so a new field cannot be silently defaulted into one of them.

## Child linking

### CLI flags

Two new flags are added to `tddy-coder` (`run.rs`):

- `--stack-parent <orchestrator-session-id>` — sets `Changeset.orchestrator_session_id` on the child, recording which session owns the stack.
- `--stack-base <base-session-id>` — sets `SessionMetadata.previous_session_id` and derives the integration base ref, then calls `spawn_chain_child_worktree`. Defaults to the orchestrator session when omitted (linear stack case where the orchestrator's branch is also the base).

### Transport-agnostic spawn helper

`spawn_chain_child_worktree(sessions_root, parent_session_id, child_session_dir, child_repo, explicit_base)` lives in `packages/tddy-session-worktree/src/session_chain.rs`. It lifts the resolve-and-integrate logic that was previously embedded in `telegram_session_control.rs::merge_chain_integration_base_with_explicit_operator_overrides`, which becomes a thin wrapper. The spawn path calls this helper with the node's derived effective base ref, which may point at a sibling session's branch rather than the orchestrator's own branch.

### Two distinct links per child

Every child session carries:

1. `SessionMetadata.previous_session_id` (+ `Changeset.worktree_integration_base_ref`) — the **base-branch source**; in a DAG this is the sibling node whose branch the child builds on.
2. `Changeset.orchestrator_session_id` — the **coordinating orchestrator**; always the orchestrator session regardless of which sibling supplied the branch base.

## Planning a stack

A stack is planned node by node, against its project record: the operator adds planned PRs in the [project stack view](#opening-a-project-stack-by-id) ("+ New planned PR", or "Add & start session"), and an orchestrator-role agent calls `work_item_add`, `work_item_update`, `work_item_set_parents`, `work_item_delete` and `work_item_adopt`. Every writer validates the same invariants on the stored `Stack` — node ids are server-assigned, every parent resolves to an existing node, and the parents never form a cycle (`Stack::topo_order`).

### PR boundary contract: every node is self-contained

A planned PR must be **independently reviewable and independently mergeable**: the API/schema change, the code implementing it, and its tests land in **one** node. A reviewer can judge it without waiting for a later node in the stack.

**Splitting by layer is forbidden.** These pairs are one node, never two:

| ✗ Layer split (invalid) | ✓ One self-contained node |
|---|---|
| `n1` add proto RPCs → `n2` implement them | `n1` attachment staging: proto + daemon handler + tests |
| `n1` add an endpoint → `n2` add its handler | `n1` the endpoint, serving real responses |
| `n1` add a data model → `n2` persist it | `n1` the model with its persistence |
| `n1` change a signature → `n2` fill in the body | `n1` the working function |

A node that ships only **surface** — RPCs returning `unimplemented`, a field nothing reads, a trait with stub impls — is not a valid PR. It cannot be reviewed for correctness (there is no behavior to check), it cannot be tested beyond compiling, and it leaves a contract in the tree that misrepresents what the system does.

**When a vertical slice is too large, split by capability, not by layer.** Cut along user-visible increments where each part is still end-to-end: one source variant rather than all of them, one scope/enum case rather than the full set, one screen or entry point, the happy path before the edge cases. Each such PR carries its own contract *plus* behavior *plus* tests, and the next node extends it.

**Two narrow exceptions** — a node may omit implementation when it is a purely mechanical rename/move/extraction with no behavior change, or a regeneration of already-committed generated code exposing no new surface. Anything else goes in the node's `description` for a human to decide; the agent is told not to invent a third exception.

This contract is **advisory, not machine-enforced**: whoever plans the stack — the operator in the stack view, or the orchestrator agent calling the stack tools — applies it.

**Why there is no validator for it.** A stack writer sees only `node_id`, `title`, `description`, `branch_suggestion`, and `parents` — never the diff a node will eventually produce — so it cannot distinguish a vertical slice from a layer split. Any check reduces to a keyword heuristic over `description` ("reject plans mentioning *proto only*"), which is trivially reworded around and would reject legitimate plans. Enforcement was considered and rejected on those grounds; the node `description` is the escape hatch that surfaces a debatable boundary to a human reviewer. Being guidance, it shapes planning without guaranteeing it: if layer-split stacks keep appearing in practice, the next step is a **plan-review gate**, not a regex in a validator.

## PR-management tools

An orchestrator-role agent has a set of `pr_*` tools that let it manage the stack explicitly, advertised automatically via the session's MCP config and auto-allowed by the permission `decide()` (all `mcp__tddy-tools__*` are allowed without a prompt).

The ten plan and read tools — `work_queue_list`, `work_queue_status`, `work_pr_read`, `work_pr_search`, `work_pr_comments`, `work_item_add`, `work_item_update`, `work_item_delete`, `work_item_set_status`, `work_item_adopt` — are **daemon exec tools** (`tddy-tool-engine::work_queue_tools`, served by `tddy-daemon-rpc` over the host's `WorkQueueStore`). Each addresses a queue by `queue_id` — the owning session's id — and a call that names none acts on the **caller's own queue**; `daemon_instance_id` forwards the call whole to the host that holds the queue. There is no creation tool: a queue exists only because its session was started with one. `work_queue_list` answers the project's queues on the host, each with its owning session, title, holding host and node count. GitHub calls use the **project account's** token, not the calling session's. A read-only session may call them: they write only the queue and, for `work_item_update` with `sync_pr`, the PR's title and body on GitHub. `tddy-tools` keeps no copy of these tools; it advertises them from the daemon's `ListExecTools` and forwards calls to it.

The six git tools — `work_item_merge`, `work_item_repoint`, `work_item_close`, `work_item_resolve_conflicts`, `work_item_set_parents`, `work_item_spawn_session` — are **daemon exec tools** too, in the sibling family `tddy-tool-engine::work_item_git_tools`, served by `tddy-daemon-rpc` over the same store and addressing: each names a `node_id` (and accepts `queue_id` and `daemon_instance_id`, forwarding whole to the holding host).

**Access follows the queue, never a role** (`tddy-tool-engine::work_queue_access`). For one queue, a caller is its **Owner** when it is the queue's session — every plan, read and git tool — or **Addressed** when an item of the queue links to it (`WorkItem.session_id`, or one of the item's `linked_sessions`) — `work_queue_status` and `work_pr_read` only; any other session has no access. A call that acts on a queue is checked on the host that holds it, against that queue (`WorkQueueToolPort::access_to` / `WorkItemGitToolPort::access_to`, backed by `caller_records::access_to_queue_here`): a queue id naming a session that owns no queue is refused with *"session <id> has no work queue — a queue is created when a session is started with a work queue"*, and a queue that is not the caller's is refused as such. A call that names a queue held by another host is forwarded there by the caller's own host **without** a local check (`names_a_queue_on_another_host`), since that host holds no record of the queue. A call that acts on no queue (`work_queue_list`, `work_pr_search`, a read by pull number) is served to a caller whose own records give it any access, checked on the caller's own host. **Listing follows the same rule:** `caller_records::work_queue_caller_from_records` reads the caller's best access over its project's queues on its own host (Owner if it owns one, else Addressed if an item links it), and both `ListExecTools` paths — the daemon's catalogue and the jail bridge (`list_exec_tools_from_jail` in `tddy-session-lifecycle`) — answer the session's role catalogue with its work tools replaced by exactly what that access offers (`with_work_tools_offered_to`). A session whose records cannot be read is refused the work tools.

**A session on another host is served by the host that holds the queue.** A forwarded work-tool call is confirmed with the session's owner first; the holding host then decides access by the queue it holds, as above. A call acting on no queue is not re-checked there: the caller's own host checked it before forwarding. One listing gap remains: a session addressed only by a queue held on another host is not *listed* the two reads by its own host's `ListExecTools`, because that host holds no record of the queue — it can use the reads only by addressing the call at the host that holds the queue. Every git step runs on the daemon **in the node's own worktree** — the worktree of the project's repository that holds the node's branch — never in the caller's checkout. A rebase is refused, untouched, while that worktree has uncommitted changes or the node's session is running, then force-pushes with lease to the project's remote with the project account's token. `work_item_resolve_conflicts` finds the conflicting paths with `git merge-tree --write-tree`, leaving the worktree exactly as it was, marks the node `has-conflicts`, and with `delegate: true` asks the node's session to resolve them — `SendSessionMessage` when it is running, `ResumeSession` with the message when it is stopped. `work_item_spawn_session` starts the node's session through the daemon's start path with the caller as its stack parent, defaulting `session_type` and `model` to the caller's own, and binds the node to the started session. `tddy-tools` keeps no copy of the six; it advertises them from the daemon's `ListExecTools` and forwards calls to it.

| Tool | Purpose | Reuses |
|------|---------|--------|
| `work_queue_list` | List the project's queues on a host, each with its `queue_id` (the owning session), title, holding host and node count. | `WorkQueueStore::list` |
| `work_queue_status` | List every node with its live GitHub state (`PrLiveStatus`) and its computed [internal status](#internal-pr-status); writes derived statuses back to the changeset. | `assemble_views`, `effective_base_ref`, `PrLiveStatus` |
| `work_item_merge` | Merge a node's PR into its base, as the project account. | daemon `DaemonStackGitTools`, `WorkQueueOpJournal` beside the stack |
| `work_item_repoint` | Rebase a node's branch onto its effective base (or a named base) in its own worktree and repoint its PR base branch after an ancestor merges. | daemon realign: `git_ops::rebase_onto` in the node's worktree, force-push with lease, `patch_pr_base` |
| `work_item_close` | Close a node's PR without merging, as the project account. | daemon `DaemonStackGitTools` |
| `work_item_resolve_conflicts` | Return the paths a node's branch conflicts on against its base, probed without touching its worktree; marks the node `has-conflicts`. With `delegate`, the node's session is asked to resolve them. | `git merge-tree --write-tree`; `SendSessionMessage` / `ResumeSession` |
| `work_item_set_status` | Agent override: set a node's internal status `kind` + `note` with `source = "override"`. | `update_stack_atomic` |
| `work_item_add` | Append a planned PR node mid-flow. Additive only — it never touches an existing node; editing one is `work_item_update`. | `item_ops::add_work_item` |
| `work_item_spawn_session` | Start a child coding session for a node (with the caller as `work_queue_parent`) — the same effect as the web "Start session" CTA, driven from chat. | daemon start path (`StartSession` through `DaemonProjectHome`) |
| `work_item_update` | Edit a node's `title` / `description` (any time, including once it owns a branch, a session and an open PR) and its `branch_suggestion` (only while it owns no branch). Opt-in `sync_pr` also publishes the new title/body to the node's PR. | `item_ops::{update_work_item, sync_node_to_github_pr}` |
| `work_item_delete` | Remove a node, reparenting its children onto that node's parents. Refuses a node whose PR is open. Branch, worktree and child session are left alone and reported back as unowned. | `item_ops::delete_work_item` |
| `work_item_set_parents` | Give a node a whole new parent list — the plan-level move, and the only reorder primitive (stack order is derived from `parents`). When the node owns a branch it is realigned exactly as a repoint realigns it. | `item_ops::set_work_item_parents` |
| `work_pr_read` | One PR in full: title, body, state, base/head, mergeability, one latest review state per reviewer, and the head commit's check runs. Changed files only on request. | `pr_insight::read_pr` |
| `work_pr_search` | Find PRs in this repository — including ones the stack does not track — by text, state, author or base. | `pr_insight::search_repository_prs` |
| `work_pr_comments` | A PR's review feedback: submitted reviews, diff-anchored comment threads, and conversation comments. | `pr_insight::read_pr_comments` |
| `work_item_adopt` | Create a node from an existing PR, bound to its head branch and PR reference. | `item_ops::adopt_pr_as_work_item` |

`item_ops`, `assess` (`assemble_views`, `effective_base_ref`, `decide_next_action`) and `pr_insight` are modules of the [`tddy-work-queue`](../../../packages/tddy-work-queue/docs/architecture.md) crate; so are `internal_status` (derivation and reconciliation), `op_journal` (`WorkQueueOpJournal`, `MergePhase`, `recover_in_flight_stack_op`) and `merge_repoint` (`execute_stack_merge`, `execute_stack_repoint`).

Merging and repointing keep their prior crash-safety semantics (`WorkQueueOpJournal`, idempotent repoint, `--force-with-lease`), entered when the agent calls `work_item_merge` / `work_item_repoint`.

### Full control over the plan *(added 2026-07-30)*

The plan stays editable after the stack becomes real: a node can be edited, deleted or moved in the DAG once it owns a branch, a session and an open PR, and the agent can read a PR in depth, search the repository's PRs and bring an existing PR in.

Rules that are not obvious from the table:

- **`work_item_repoint` and `work_item_set_parents` are different operations.** Repointing answers *"the base branch drifted — retain the parent that owns this target"*; setting parents answers *"the plan changed — this node belongs here now"*, with the caller naming the complete new set (an empty list makes the node a root). They share one git+GitHub tail, `realign_node_to_effective_base`: rebase onto the new effective base, `--force-with-lease` push, re-target the open PR.
- **Deleting reparents; it never cascades.** Children inherit the removed node's parents, deduplicated, and the removed node's own id is filtered out of what they inherit. This is not politeness: [`Stack::topo_order`](#stack-data-model) counts in-degree only over parents that resolve to a node, so a delete that merely dropped the node would leave the stack quietly describing an edge that no longer exists, and nothing would ever report it.
- **A rejected mutation writes nothing.** All four writers validate against a candidate `Stack` — existence, self-parenthood, repeated ids, cycles — before calling `update_stack_atomic`.
- **A node's PR is still identified by `pr_status.url`.** `work_pr_read` / `work_pr_comments` / `work_item_adopt` resolve a pull number through `pr_number_from_status_url`; `WorkItem` gained no PR-number field, so there is one source of truth and no migration. A node that records no URL is not addressable by `node_id` and says so rather than guessing.
- **Adoption cannot double-track a PR.** It is refused when the PR's head branch is already bound to a node *or* when any node already records that pull number.

Two limitations are part of the contract rather than defects:

- **A search hit carries no head or base branch.** `GET /search/issues` does not report them, so `base:` works as a search qualifier but the branch names are absent; the agent follows up with `work_pr_read`.
- **No thread is reported as resolved.** Thread resolution is exposed only by GitHub's GraphQL API, so the REST-backed `work_pr_comments` emits no `resolved` field rather than guessing one.

A search is always scoped to the orchestrator's own repository: `repo:` and `is:pr` are injected by the tool, and the agent's `text` / `author` / `base` values are refused if they carry a `:` or whitespace — GitHub *ORs* repeated `repo:` qualifiers, so an unguarded value would read another repository with the operator's own credential.

### An item's assigned PR and linked sessions *(added 2026-11-22)*

A work item can be **assigned an existing pull request** and **linked to any number of tddy sessions**
(on any host of the project), beside the session it was started into (`WorkItem.session_id`). The four
link tools — `work_item_assign_pr { item_id, pull_number }`, `work_item_unassign_pr { item_id }`,
`work_item_link_session { item_id, session_id }`, `work_item_unlink_session { item_id, session_id }` —
are daemon exec tools in `tddy-tool-engine::work_item_link_tools`, offered to the queue's **owner** only
(addressed sessions get none of them), and the same four operations are RPCs on
`work_queue.WorkQueueService` (`AssignWorkItemPr`, `UnassignWorkItemPr`, `LinkWorkItemSession`,
`UnlinkWorkItemSession`, each answering the queue after the change) behind the queue row's controls.

- **Assigning reads the PR, as the project's account**: the caller gives only the number; the item records
  the PR's own number, URL and head branch (`WorkItem.pr`), never a client's claim. The PR's head branch
  becomes the item's branch; an item that already records a *different* branch is refused, naming both —
  never overwritten. Assigning never edits the PR on GitHub.
- **Unassigning** removes the PR and keeps the branch. **Linking** the same session twice links it once;
  **unlinking** a session that is not linked is refused.
- **A linked session is Addressed** by the item's queue (it may call `work_queue_status` and
  `work_pr_read`); writing stays the owner's.
- **A refused edit writes nothing**: every link edit is applied to a copy under the queue's record lock.

**Live checks on the row.** `StreamWorkQueueStatus` answers, per item, its PR's state and the count of
its checks at the PR's head that are running, failed and succeeded, with each run (name, status,
conclusion, times, details link) for the row's expanded list — a snapshot when the view opens, then
a new one whenever the daemon's GitHub inbox reports a PR or checks change in the queue's repository. One
stream per open queue view, never per row, and the browser never polls GitHub. An item whose PR or checks
the inbox cannot read has no summary, rather than zeros. The stream is served only by the host that holds
the queue (`docs/dev/todo/2026-11-22-work-queue-status-stream-is-local-only.md`).

**PR reads go through the inbox.** `GetPrStatus`, `QueryBranch`'s PR leg and `work_queue_status` read
each branch's PR through the daemon's GitHub inbox with the queue's project credential, so a second
reader of the same branch costs no GitHub request; a read the inbox cannot make is *unavailable* with its
reason, never "no PR". The inbox keeps one reader per repository, so two projects of one repository on
one host share cached rows (`docs/dev/todo/2026-11-22-github-inbox-reads-a-repository-with-one-projects-credential.md`).

### Labels and badges on a work item *(added 2026-11-22)*

A work item carries the agent's and the user's own annotations, apart from the derived internal status
(`needs-repoint`, `has-conflicts`, …), which tddy owns and which they never touch:

- **Labels** — free-form tags (`WorkItem.labels`), several per item, shown as chips; the queue view's
  label selector filters the list to the items carrying the chosen label. Setting labels **replaces** the
  set; each label is trimmed, blanks are dropped and duplicates collapse, in the order given.
- **Badges** — keyed markers (`WorkItem.badges`: `key`, `text`, `tone`) with a tone of `info`, `success`,
  `warning` or `danger`. Setting a badge under an existing key **replaces** it; clearing a key that is
  not set changes nothing.
- **Limits are refusals, never truncation**: at most 16 labels of 40 characters, 8 badges, 60 characters
  of badge text — an over-limit call writes nothing and names the limit.

The three tools — `work_item_set_labels { item_id, labels }`, `work_item_set_badge { item_id, key, text,
tone }`, `work_item_clear_badge { item_id, key }` — are daemon exec tools in
`tddy-tool-engine::work_item_annotation_tools`, offered to the queue's **owner** only, and the same three
operations are RPCs on `work_queue.WorkQueueService` (`SetWorkItemLabels`, `SetWorkItemBadge`,
`ClearWorkItemBadge`, each answering the queue after the change; an unknown item is *not found*, a bad
tone or an over-limit call *invalid argument*). An expanded row edits its labels (comma-separated, saved
as the whole set) and clears a badge by key. A change to an annotation records no activity.

### A queue's lineage: line edges and dependency edges *(added 2026-11-23)*

A queue's structure has **two kinds of edge**, never conflated:

- **Line edges** — the branch chain on GitHub. Each item's line endpoint is read from its open PR's base
  branch through the daemon's GitHub inbox: another item's branch (`item`), the project's trunk
  (`trunk`), or a branch outside the queue (`external`). They are **never stored** — retargeting a PR on
  GitHub moves its line edge with no write to the queue. An item without a readable open PR shows its
  planned base (`parents`' first entry, or the trunk for a root); when the project's GitHub cannot be read
  at all the answer carries the planned bases and says why (`unavailable_reason`).
- **Dependency edges** — what needs what *behaviour* (`from → to`: `to` needs `from`). Stored on the item
  (`WorkItem.needs`, omitted when empty), set by the agent or the user, and **acyclic**: a set that would
  close a cycle, or names an unknown item, is refused and writes nothing; the refusal names the cycle. A
  need named twice is stored once. These are the only edges later analysis (waves, readiness) reads.

`parents` keeps its git meaning (where a branch is cut and rebased); it is not migrated into dependencies.

The owner's tools `work_queue_set_dependencies { item_id, needs }` and `work_queue_graph`
(`tddy-tool-engine::work_queue_lineage_tools`) and the RPCs `SetWorkQueueDependencies` /
`GetWorkQueueGraph` on `work_queue.WorkQueueService` serve them. Where the host's `gh` has the
`gh stack` extension, the graph also carries the checked-out branch's stack membership
(`gh_stack_branches`, read from local tracking); without it nothing else is missing.

The Work queue tab draws both under the item list (`WorkQueueGraph`): line edges solid along the branch
chain, dependency edges dashed, an external base as an endpoint outside the queue. The graph is read when
the view opens and after each queue edit; a retarget on GitHub shows on the next read
(`docs/dev/todo/2026-11-23-work-queue-graph-is-not-live.md`).

### Waves, the derived line and readiness *(added 2026-11-23)*

From a queue's dependency edges tddy derives — and never stores — three things:

- **Waves** — wave 1 is every item that needs nothing; wave k is every item whose needs all sit in waves
  below k. An item with no needs is wave 1 wherever it sits. Edges that close a cycle report the cycle
  instead of waves.
- **The derived line** — the waves concatenated; inside a wave the blockers lead (most transitive
  dependents first, ties in the queue's own order). It is the order a branch chain should have.
- **Readiness** — an item's *binding* needs: the unmet ones in the wave immediately below it (deeper needs
  are met before those, so naming them would misreport the block). A need whose item has **merged** — the
  stored PR status or derived status says merged, or the live PR read through the inbox does — is met: it
  leaves the list and the item may read "ready". A merged item keeps its wave.
- **Line check** — whether the queue's own order steps back a wave, and the first item out of place.

`work_queue_waves` (owner tool) and `GetWorkQueueWaves` answer all four; `work_queue_status` and
`StreamWorkQueueStatus` carry each item's `wave` (0 when the edges close a cycle) and `binding_needs`. The
Work queue tab draws the items in wave lanes with their readiness text and a banner naming the first item
out of place (`WaveLanes`). Nothing here reorders a branch or a PR.

### Commit inbox and outbox *(added 2026-11-23)*

For every item on a branch tddy reads three commit lists from the remote tips, after a fetch, without
touching any checkout, index or working tree:

- **Commits** — the item's own: what its branch has beyond its line parent.
- **Inbox** — what its line ancestors have that it lacks, such as commits pushed to an ancestor after the
  item was cut. Each commit is attributed to the nearest line ancestor that carries it, or to the trunk or
  external base; base sources are listed first. Each commit carries a **conflict prediction** made for
  *this* item (`git merge-tree` onto its tip), because the same commit can apply cleanly on one item and
  conflict on the next. A predicted conflict names the files.
- **Outbox** — the mirror: per successor, what this item has that the successor lacks. An item's outbox is
  always its successors' inbox entries sourced from it.

Line parents come from each PR's base through the GitHub inbox. When that cannot be read, the planned
bases stand in and the answer says why (`unavailable_reason`). Answers are cached per item by the remote
tips they were read at. The PR view (from a work item, or a session on an item's branch) shows **Inbox**
and **Outbox** tabs next to Commits, and each queue row shows its inbox/outbox counts and a conflict
marker. Agents use `work_item_inbox` and `work_item_outbox`; the RPCs are `GetWorkItemInbox`,
`GetWorkItemOutbox` and `GetWorkQueueCommitCounts`. Nothing here pushes or merges.

### Leaked commits and non-linear items *(added 2026-11-23)*

Two checks per work item with an open PR, read from the GitHub inbox and the queue owner's clone (remote
tips, after a fetch; nothing local is touched):

- **Leaks** — commits in the item's PR that belong to one of its line ancestors: the same SHA, or a
  rebased copy (same author, author date and subject). Each leak names the nearest ancestor carrying it.
  It usually means a rebase used the wrong upstream, so the PR's diff carries an ancestor's work.
- **Linearity** — whether the item contains its line parent's tip: **linear**; **behind by N** (the parent
  has N commits the item lacks); **conflicting** (behind, and GitHub reports the PR as not mergeable); or
  **unknown** (the parent merged or is only planned, or no reading is possible).

Each row shows a leak chip and a linearity chip from the status stream; `work_queue_status` returns both
per node as `chain_health`. Nothing here repairs, rebases or edits a PR.

### GitHub API surface

`GithubPrApi` trait (real implementation + mock transport for tests): `get_open_pr`, `merge_pr(number)`, `patch_pr_base(number, new_base)`, `create_pr(head, base, title, body)`, and `close_pr(number)`. Backed by shared curl helpers `curl_github_patch_json`, `curl_github_post_json`, and `curl_github_put_json` in `github_rest_common.rs`.

`GithubPrInsightApi` *(added 2026-07-30)* is a **sibling** trait, not more methods on `GithubPrApi`: `get_pr`, `list_pr_files`, `list_check_runs`, `list_reviews`, `list_review_comments`, `list_issue_comments`, `search_prs`, and the one write `patch_pr_title_body`. Kept separate because the eight hand-written fakes implementing `GithubPrApi` care only about lifecycle operations and would otherwise all need stubs for reads they never exercise. `RealGithubPrApi` implements both. `/search/issues` is not under `/repos/{owner}/{repo}/`, so `github_rest_common.rs` gained `curl_github_get_json_absolute_path` alongside the repo-scoped helpers.

Response shaping lives in `tddy_work_queue::pr_insight` (`read_pr`, `read_pr_comments`, `search_repository_prs`, `pull_number_for_node`), so every shape the agent sees is testable against a fake; the `tddy-tools` tool bodies only resolve environment and serialize. The `curl` request bodies themselves have no automated coverage — the transport shells out to `curl` against a hardcoded API base and cannot be intercepted, the same gap the `GithubPrApi` methods have.

## Internal PR status

`internal_status: Option<PrInternalStatus>` on `WorkItem` is the **action-needed** signal, orthogonal to `pr_status` (which mirrors GitHub reality):

```
PrInternalStatus { kind: String, note: Option<String>, source: String }
```

- **`kind`** — one of `up-to-date`, `needs-repoint`, `has-conflicts`, `ready-to-merge`, `blocked`, `merged`.
- **`note`** — optional free-text annotation (agent context, e.g. "waiting on API design").
- **`source`** — `derived` (auto-computed) or `override` (agent-set).

**Derivation** (in `work_queue_status`, from `NodeView` / `PrLiveStatus` / `effective_base_ref`):

1. PR merged → `merged`.
2. A parent has merged but the node's PR base ≠ its effective base → `needs-repoint`.
3. Syncing the branch with its base surfaces unmerged paths → `has-conflicts`.
4. PR open, all deps merged, no conflicts → `ready-to-merge`.
5. Otherwise → `up-to-date`.

**Override wins:** a node whose `source == "override"` is **not** overwritten by derivation — the agent's manual status (e.g. `blocked` with a note) persists until it clears the override. This is auto-derived + agent override, per the design decision.

`internal_status` is additive (`#[serde(default, skip_serializing_if = "Option::is_none")]`) so old `changeset.yaml` files deserialize with `None`. It rides to the web inside `SessionEntry.work_queue_json` (no proto change) and renders as a colored badge on each planned-PR row (§ Web UI).

## Merge and repoint

When the agent calls `work_item_merge` the daemon merges via the GitHub REST API, as the project account, on the daemon (`work_item_merge`). For each dependent node of the just-merged node:

1. **GitHub base update:** `patch_pr_base(dependent_pr_number, new_base)` where `new_base` is the recomputed effective base (now `main` or the next non-skipped ancestor).
2. **Git history repoint:** on the daemon, in the worktree that holds the dependent branch: `git rebase --onto <new_base> <old_base> <branch>`, then a force-push with lease to the project's remote. A `git merge-base` fallback guards against a stale `<old_base>`. A worktree with uncommitted changes, or a node whose session is running, is refused untouched.

Rebase/merge conflicts are surfaced to the agent via `work_item_resolve_conflicts`: the daemon probes the branch against its base with `git merge-tree --write-tree` — no merge is started, so the worktree is left exactly as it was — marks the node `has-conflicts`, and returns the conflicted file list. With `delegate: true` the node's own session is asked to resolve them; otherwise the agent resolves them and re-runs the tool to confirm a clean tree.

## Full DAG handling

GitHub PRs have a single base ref, so a node that depends on multiple unmerged parents requires special treatment:

- The orchestrator maintains a local `stack-int/<node_id>` integration ref, produced by merging all non-skipped parent branch tips. The node's branch is created or rebased onto this integration ref.
- The **GitHub PR base** points at the **first** non-skipped parent (the primary spine). Commits from the other parents arrive via the integration ref merge.
- As parents merge to the main branch, `effective_base_refs` shrinks. The integration ref is refreshed when the effective parent set changes.
- A multi-parent node's PR is only offered for merge once **all** its parents are merged (so its effective base collapses to the main branch, matching step 2 of the `assess` algorithm).

### Operator-selectable base branch for a diamond node *(added 2026-07-27)*

A planned PR with multiple non-merged parents (a diamond / merge node) used to base its child worktree off whichever parent the resolver walked **first** — `Stack::base_ref_for_spawn` / the web `resolveWorkItemBase` iterate `node.parents` in list order and return the first non-merged ancestor's `origin/<branch>`. Every parent is a legitimate base ref, but the operator had no control to pick a different one, and the dialog's `baseBranchLabel` was display-only (never sent to the daemon).

The Start-session dialog opened from a planned-PR row now renders a **"Base branch"** `<select>` (test id `create-session-base-branch-select`), shown when `initialValues.stackParent` is set and the option list is non-empty. Options and pre-selection are resolved together by **`baseBranchChoice(node, nodes, defaultBranch): { options, selected }`** (`packages/tddy-workqueue-web/src/`), which orders the stack's own branches via the pure helper **`prioritiseBaseBranchOptions(node, nodes): string[]`**:

1. **Direct dependency branches** — walk `node.parents` in order; for each parent that is **not merged** and **owns a `branch`**, include that branch. Order the result by the dependency's own depth in the stack DAG (longest path from a root, deepest first), ties broken by the order in `node.parents` (stable). A merged parent contributes nothing (its ref may be gone and `effective_base_refs` collapses past it); a branchless parent contributes nothing (no ref to offer — the spawn gate would refuse it anyway).
2. **Other materialized stack branches** — every node with a `branch`, excluding the node itself, its descendants (basing onto a descendant would create a cycle), merged nodes, and any branch already listed in step 1. Appended in stack node order. De-duplicated by branch name (first-seen wins).
3. **The project's default branch** — appended by `baseBranchChoice` itself, last (from `ProjectEntry.main_branch_ref`), unless a stack branch already names it. It is the deliberate escape from the stack: a node **repointed onto the default branch** must be able to show that base and re-pick it. *(added 2026-07-30)*

The selected value is the node's **derived stack base** — `deriveStackBaseBranch`, the same value the branch-intent caption states — and is sent on the existing **`StartSessionRequest.selected_integration_base_ref`** field. It is always one of the offered options, so nothing can be submitted that the operator cannot see selected. *(Until 2026-07-30 the pre-selection was `options[0]` and the default branch was not offered at all: a planned PR repointed onto the default branch showed the caption `origin/master` while the picker pre-selected — and submitted — an unrelated stack branch, silently undoing the repoint.)* When the stack offers no branch of its own (a root node with no other materialized branches) the selector is hidden and the field is sent empty — preserving the root → default-branch behavior, which the daemon resolves for itself. A legacy project that stores no `main_branch_ref` still gets the default option, as the empty ref under the label *"project default"*.

**Ordering rationale — depth, not distance.** For a diamond where `PR3` depends on `[PR2, PR1]` and `PR2` depends on `PR1`: both direct parents are at distance 1, but `PR2` sits **deeper** in the DAG (itself depends on `PR1`). Basing onto `PR2` gives the operator `PR1`'s changes too, so `PR2` is the more specific, more complete base and is listed first. Depth is the longest path from a root to the dependency; ties (two roots, as in the `attach-start` case) break by `node.parents` order.

The daemon honors the explicit choice at the spawn seam — see [Git integration base ref — Operator-chosen base at the spawn seam](git-integration-base-ref.md#operator-chosen-base-at-the-spawn-seam-added-2026-07-27). The existing `baseBranchLabel` (the branch-intent caption, from `deriveStackBaseBranch`) is unchanged and independent of the selector.

## Resumability and crash safety

**Loop resumability** is free: `assess` is idempotent, and every non-terminal state maps to `assess`. Restarting the orchestrator session re-enters `assess` exactly as if the previous tick completed.

**Merge+repoint atomicity:** a transient journal file records the in-flight operation — beside the queue, in its owning session's directory (`<sessions>/<queue_id>/work-queue.op.json`), for an operation a daemon work git tool started, and at `parent_dir/.workflow/stack-op.json` for a pr-stack session's own operation, whose in-flight recovery is unchanged:

```
WorkQueueOpJournal { op_id, merged_node_id, merge_phase, pre_op_snapshot, dependents }
```

`merge_phase` transitions: `Planned → PrMerged { sha } → RepointingDependent { idx } → Done`. Each transition is an atomic rename.

A recovery guard (`tddy_work_queue::op_journal::recover_in_flight_stack_op`) checks for an in-flight journal: if the phase is `>= PrMerged` but not `Done`, the orchestrator verifies the merge via GitHub and **resumes repointing** (never re-merges). If the phase is `Planned` and the PR is still open, the operation can be safely retried or aborted. All repoint steps are idempotent: `patch_pr_base` to the correct base is a no-op; `rebase --onto` with the merge-base guard re-runs cleanly; `rerere` replays conflict resolutions.

`--force-with-lease=<branch>:<expected-sha>` ensures that a concurrent child push aborts the repoint and routes to `MarkFailed` rather than silently clobbering work.

## Web UI: session creation and drawer grouping

### `orchestrator_session_id` in proto

`Changeset.orchestrator_session_id` (a child-session back-reference to the orchestrating session) is surfaced to the web via `SessionEntry` proto field 21 (`orchestrator_session_id: string`). It is populated during enrichment by `session_list_enrichment.rs` reading the child's `changeset.yaml` (alongside `changeset.state.current`). Empty string for non-child sessions.

### Context docs in proto

The session's **context documents** are surfaced to the web via `SessionEntry` proto field 27, `repeated SessionContextDoc context_docs`, where `SessionContextDoc { key, basename, path, description, exists, kind, size_bytes }` and `kind` is `SessionContextDocKind` (`MANIFEST` or `ATTACHMENT`). Enrichment populates the list in `session_list_enrichment.rs` (alongside `orchestrator_session_id`):

- **Manifest rows** — recipe-owned planning artifacts (e.g. `exploration.md`). `session_context_docs::context_docs_for_session` joins the manifest's `known_artifacts()` with a per-key human `description` (`SessionArtifactManifest::artifact_doc_descriptions()`), an on-disk existence flag, and `size_bytes`, resolving each `path` under `session_artifacts_root` (`session_dir/artifacts/`). A blank or unknown recipe yields an empty *manifest* half.
- **Attachment rows** — user-attached files under `artifacts/attachments/`, listed after the manifest rows with `kind = ATTACHMENT`. Attachments are recipe-independent: a blank or unknown recipe still lists them. Layout and store contract: [session-attachments.md](session-attachments.md).
- `session_context_docs::read_session_context_doc_utf8(recipe_name, session_dir, basename)` reads a doc's contents, allowlisted to the **manifest** basenames with a canonicalize-and-contain guard rooted at the artifacts dir (a non-manifest / attachment basename or a traversal segment → `PermissionDenied`). It mirrors the guard shape in `session_workflow_files.rs`. The wire RPC that exposes this reader is added by the web-facing follow-up that consumes it (the **Docs** tab and child "Start session" prompt references).

### New-session screen: recipe dropdown

`CreateSessionPane` (`packages/tddy-session-create-web/src/`) lists the canonical recipe set in a `<select>` for tool sessions; no stack recipe is among them, and the form offers no stack picker of its own. A child of a planned PR is started from that PR's row in the stack view, which opens the form with `stackParent` and `workItemId` already named (`initialValues`); the form sends them as `work_queue_parent` (proto `StartSessionRequest` field 15) and `work_item_id`, and the daemon threads them through `SpawnOptions` → `--stack-parent <id>` / `--stack-node-id <id>`. The host that owns the stack parent is found from the session list.

### Planned-PR rows

Each planned-PR row (`WorkItemRow.tsx`) renders an **internal-status badge** next to the existing phase chip, colored by `internal_status.kind` (e.g. amber `needs-repoint`, red `has-conflicts`, green `ready-to-merge`), with `internal_status.note` as hover text. The badge is parsed from the `internal_status` field carried inside `SessionEntry.work_queue_json` (`stackPlan.ts::parseWorkQueue`).

> **Updated 2026-08-01 — the row is a control surface.** A row now collapses to a summary and expands to its full detail; the spawned indicator opens the child session it is bound to; row order is a **persisted `WorkItem.display_order`** with move up/down rather than being re-derived from `parents` (so a merge or a repoint no longer moves a row the operator is reading); each row states how its branch stands against its base, computed non-mutatingly on the `QueryBranch` poll; and a row cleanly behind its base can **pull the base in** (merge by default, rebase on request). Note this makes `has-conflicts` a **live** fact rather than only an agent-set `internal_status` — the derivation table in `internal_status.rs` still never produces it, and the pull path deliberately does **not** stamp it, since a persisted copy could only go stale and clearing it would risk stomping an agent's `source: "override"`. See [PR-Stack live status § Panel UX](pr-stack-live-status.md#panel-ux-expandable-rows-session-links-persisted-order-base-sync-added-2026-08-01).

### Manually adding a planned PR

The stack view's planned-PR list offers a **direct, deterministic path** that doesn't touch an LLM: a "New planned PR" form lets the operator manually add a single node — title, description, optional branch suggestion, optional child recipe, and a **multi-select ancestor picker** listing the orchestrator's existing planned-PR nodes (its chosen ancestors become the new node's `parents`, i.e. `WorkItem.parents` — see [Stack data model](#stack-data-model)).

- **RPC:** `work_queue.WorkQueueService.AddWorkItem(AddWorkItemRequest) -> AddWorkItemResponse` (`work_queue.proto`). Request carries `session_id` (the orchestrator), `title`, `description`, `branch_suggestion`, `parents` (chosen ancestor node ids), and `child_recipe`. Response carries `work_queue_json` — the same wire shape as `SessionEntry.work_queue_json` (field 23) — so the web reuses the existing `parseWorkQueue` parser rather than a second message schema, plus `node_id` (field 2), the id of the node this call appended (see [Add and start in one step](#add-and-start-in-one-step-added-2026-08-13)).
- **Semantics:** appends exactly one `WorkItem` (`branch: None`, `session_id: None`, `pr_status: None` — stays planned, does not spawn a session or create a branch; only `branch_suggestion` is set) to the orchestrator's stack via `update_stack_atomic`. It **never touches existing nodes** and is not gated on whether other nodes have already spawned — it's additive only. The node id is server-assigned (never client-supplied). Rejects (without writing) when a chosen ancestor doesn't resolve to an existing node id, or when appending would introduce a cycle (checked via `Stack::topo_order`).
- **UI:** a "+ New planned PR" entry point on the planned-PR list opens the form; ancestors are chosen via checkboxes over the currently-listed nodes (topo order). On success the list re-renders with the new node included.

Implementation: `tddy_work_queue::item_ops::add_work_item` (pure function — read/validate/append/write); daemon handler `ConnectionServiceImpl::add_work_item` in `connection_service.rs`.

#### Add and start in one step *(added 2026-08-13)*

The "+ New planned PR" form carries two submits — **"Add planned PR"** (`work-item-add-submit-btn`) and **"Add & start session"** (`work-item-add-start-btn`). The second opens the row's own [Start session dialog](../web/session-drawer.md#start-session-cta) for the node it just appended, instead of making the operator find the new row in the list and click its CTA. Nothing new happens on the host: the same `AddWorkItem` call, then the same dialog, the same initial-values derivation (`baseBranchChoice`, `deriveStackBaseBranch`, the stack-parent wiring) and the same child-created callback the CTA already uses.

**The server names the node it created** — `AddWorkItemResponse.node_id`, populated from the `WorkItem` the daemon's handler already has in hand. The client cannot infer it by diffing the returned plan against the ids it held before the call: the orchestrator **agent** appends nodes to the same stack, and the screen's copy of the plan only refreshes on the next session-list refetch, so one response can carry several ids the screen has never seen — some appended before the operator's node and some after (one race while the form was open, one between the daemon's write and the re-read that serializes the response). Every positional pick among them is wrong in one of those cases, and the dialog it opens is pre-filled with somebody else's branch and title — which is then spawned against. A malformed plan made it worse still: `parseWorkQueue` degrading to `{nodes: []}` made *every* node look new and selected the stack's bottom node, while the defensive "no new node" refusal never fired, because a node had been found. The field is additive — a client reading only `work_queue_json` is unaffected — and the refusal is kept for the one case that is now a real inconsistency: a response whose plan does not carry the node it named leaves the list updated (the node really was appended server-side) and withholds only the session start, which the form reports in its error strip.

### Opening a session's work queue

Every `WorkQueueService` request carries a **queue address** — `WorkQueueAddress { project_id, queue_id, daemon_instance_id }` in `work_queue.proto`, where `queue_id` is the **owning session's id** — beside the owning `session_id` a request may name instead. The address wins when both are named; an address missing its project or queue id is `InvalidArgument`, never completed from the session; an empty `daemon_instance_id` names the daemon the call arrived on. A session that owns no queue is refused by the `session_id`-addressed mutations (`AddWorkItem`, `LinkWorkItem`, `ReorderWorkItem`, `RepointWorkItem`, `PullBaseIntoBranch`) with `FailedPrecondition` and *"session <id> has no work queue — a queue is created when a session is started with a work queue"* (`require_a_stack` in `packages/tddy-daemon-rpc/src/work_queue/project_stacks.rs`); no mutation creates a queue. The reads (`QueryBranch`, `GetPrStatus`, `ResolveWorkItemBase`) are served for a session that owns none. Authentication is checked before the address is validated. The daemon the address names serves the call; another host forwards it (`forward_if_addressed` in `packages/tddy-daemon-rpc/src/work_queue/project_stacks.rs`). There is no RPC that creates a queue.

Two RPCs read a project's queues by address:

- **`ListProjectWorkQueues(project_id)`** → `WorkQueueSummary { address, title, node_counts { total, started, merged }, referencing_session_ids, created_by_session_id, created_at_unix_ms }` per queue, where `created_by_session_id` and the one `referencing_session_ids` entry are the owning session (the queue's id). The listing fans out one hop: the forwarded copy carries `this_host_only = true`, every eligible peer is asked, and a peer whose forward fails is named in `UnreachableWorkQueueHost { daemon_instance_id, reason }` while the listing itself still succeeds. Node counts derive from the stored queue: `started` counts items with a session or a branch recorded, `merged` counts items whose PR status phase is `merged`.
- **`GetWorkQueue(address)`** → the summary plus the queue's `work_queue_json`, forwarded to the owning host.

Implementation: `packages/tddy-daemon-rpc/src/work_queue/project_stacks.rs` (address resolution, target and session-dir resolution) and `work_queue_queries.rs` (the two RPCs and the by-address store mutations); `ports.rs` only delegates. `SessionEntry.work_queue_id` (proto field 38) carries the id of the queue a session owns — its own id — and `SessionEntry.owns_work_queue` (field 44) whether it owns one; both are filled from the session directory in the session-list enrichment.

The **stack view** (`WorkQueueView` in `packages/tddy-workqueue-web`, `work-queue-view`) renders the Planned PRs panel full width for one stack, loaded by address through `GetWorkQueue`; every control (start session, repoint, reorder, add planned PR, pull base) names that address. The route is `#/sessions/project/<projectId>/stack/<stackId>` (`sessionsProjectQueuePath` / `parseSessionsProjectStack` in `packages/tddy-client-web/src/appRoutes.ts`, re-exported by `packages/tddy-web`); the owning host comes from the drawer's listing row, not the URL.

### Session drawer: children collapsed under the main stack session

`SessionDrawer` (`packages/tddy-session-web/src/SessionDrawer.tsx`) renders PR-stack sessions in a collapsible group rather than a flat list.

Grouping logic lives in `packages/tddy-session-web/src/utils/sessionStackGroups.ts`:

```
groupSessionsByStack(sessions) → { groups: { parent: SessionEntry, children: SessionEntry[] }[], flat: SessionEntry[] }
```

- **Child** = session with a non-empty `orchestratorSessionId` pointing at a present session.
- **Parent** = session referenced as `orchestratorSessionId` by at least one child.
- Children whose parent is absent fall into `flat` (like `isSessionOrphan` in `sessionProjectTable.ts`).
- Within each group, parent and children are sorted by `sortSessionsByCreation`; groups themselves ordered by parent `createdAt` (newest first).

`SessionDrawer` replaces its flat `sessions.map` with:
- Per group: a native `<details data-testid="sessions-drawer-stack-<parentId>" open>` whose `<summary>` contains the parent's `SessionDrawerItem` and whose body renders children with `depth={1}`.
- Then `flat` sessions with `depth={0}`.

`SessionDrawerItem` gains `depth?: number` (indentation) and a chevron indicator for group parents. The native `<details>`/`<summary>` collapse pattern reuses `ConnectionScreen.tsx:2140-2154`.

Only one nesting level is rendered for v1; the grouping utility is written to support recursion later.

## Related

- [PR-stack documents](pr-stack-docs.md) — the per-PR PRD and changeset, and how a child session is started holding them
- [PR-Stack live status & repoint](pr-stack-live-status.md) — the branch as the durable link key; branch→session/PR resolution and live PR status in the web view; the Repoint control; and the sequence-respecting spawn base (`Stack::base_ref_for_spawn`).
- [Session drawer](../web/session-drawer.md) — session drawer screen layout, create session, recipe field, grouping, and the [Per-Workflow Session Views](../web/session-drawer.md#per-workflow-session-views).
- [Git integration base ref (worktrees)](git-integration-base-ref.md) — session chaining, `spawn_chain_child_worktree`, worktree base-ref validation.
- [Session layout](session-layout.md) — session directory structure, `changeset.yaml`, artifact paths.
- [Workflow recipes](workflow-recipes.md) — `WorkflowRecipe` trait, recipe resolution, `approval_policy`, shipped recipes.

# Work-queue status shows linked sessions and the item's branch

**Type:** Fix
**Work item:** n7 (orchestrator queue of session `01a20f10-4a30-7df1-8c63-d12aec760d68`)
**Packages:** `tddy-changeset`, `tddy-tool-engine`, `tddy-daemon-rpc`, `tddy-workqueue-web`

## State A

`work_item_link_session` records its session on the item's `linked_sessions`, and answers it
back — but `work_queue_status` never reads that field. Its nodes carry the legacy single
`session_id` (written only by the spawn-link path) and the stored `branch`, so an item the
orchestrator linked to a session shows `session_id: null` and `branch: null`. Seen on the
orchestrator queue of session `01a20f10-4a30-7df1-8c63-d12aec760d68`: items n1–n6 were linked
and the status shows no session and no branch on any of them.

Three gaps make up the one symptom:

- The link tool names no host: `WorkQueueOp::LinkSession` carries `item_id` and `session_id`
  only, and the daemon stores `daemon_instance_id: ""` always — while the sibling RPC
  (`LinkWorkItemSession`) and the stored `LinkedSession` both carry the holding host.
- The status answer omits `linked_sessions` and reports the stored `branch` as-is. The one
  documented branch fallback, `resolve_stack_node_branch` (the node's own branch, else its
  child session's changeset branch), reads only the legacy `session_id`, never the links.
- The web row joins its child session by participant claim, plan record and branch ownership
  (`nodeChildSession`, D39) — never by the plan's links — and its hydration fills
  `branch`/`sessionId` from that join only. The link controls list each link's session id
  without its host.

## State B

- `work_item_link_session` takes an optional `session_daemon_instance_id` (empty for the
  queue's own host), stored on the link. `daemon_instance_id` was already taken — it names
  the queue's host — so the session's host needed its own argument.
- A link answer lists its sessions as `{ session_id, daemon_instance_id }`, not bare ids.
- `work_queue_status` lists every linked session with its host on each node, and the item's
  effective branch: the stored branch, else the first locally readable linked session's
  changeset branch (the legacy recorded child first), else the assigned PR's head branch.
  The resolved branch is display-only: it is never written back to the item.
- `resolve_stack_node_branch` falls back through the linked sessions in link order after the
  recorded child. It reads each link's session directory under the local sessions root and
  skips what is not there — no host comparison, since session ids are unique across the
  fleet and a remote link simply has no local directory. The inbox view assembly
  (`assemble_views_through_inbox`) and the spawn gate read through the same resolver, so a
  linked item's PR state and spawn base follow.
- The web join tries the plan's links after the recorded child
  (`nodeChildSessionByIdentity`, extending D39), so hydration fills the branch and session
  of a linked child that publishes no participant metadata, and the status chip, bound
  session and in-progress badge follow. The row's link list names each link's host
  (`linked: <id> on <host>`, bare id on this host).

## Decisions taken (recorded for review)

1. **Effective branch order: stored, then session, then PR.** The task names the linked
   session's branch first; the PR leg only fires for a hand-edited queue, since `assign_pr`
   persists the head branch as the item's branch at assign time.
2. **The recorded child beats a linked session** for branch resolution and for the web join:
   it is the spawn-recorded binding; links are the later fallback.
3. **No host comparison on the daemon read path.** A link's `daemon_instance_id` stays
   display/routing data; branch resolution tries the local directory and skips what is not
   there. Session ids are fleet-unique (UUIDs), so a local directory with the id is that
   session, and a remote link resolves to nothing here.
4. **No PR-head hydration on the web, and none in the gate resolver.** A hydrated
   `node.branch` feeds base resolution, the spawn gate and the poll set; only a session's
   branch (a ref that exists, D1) may feed those. The PR fallback lives only in the status
   tool's answer, which is read-only display.
5. **The status `branch` carries the effective branch**, not the stored one plus a second
   field: consumers read `branch`, and nothing writes it back.
6. **The link answer shape changes** from bare ids to `{ session_id, daemon_instance_id }`
   objects. Nothing pins the old shape (no test, no prompt), and a bare id cannot confirm a
   remote link's host.

## Status refresh credential failure (investigated, unchanged)

Observed alongside the bug: status refresh fails with `no credential vault is available to
this session start, so no account can be resolved` when an orchestrator or project-home
session asks for it.

On current code this error is caller-independent, and it is not explained by the call
carrying no browser token:

- Status refresh resolves credentials through `site_for_session` →
  `HostTrunkSource::feed_for` → `project_account_token` →
  `SessionAccountAccess::for_subject(vaults, login)`, where the login comes from verifying
  the call's session token. Agent tokens (`mint_agent`) name the operator's login and
  verify like browser tokens, so an orchestrator's call resolves the same vault a browser
  call would.
- Each neighboring failure has its own message: an unverifiable or expired token yields
  `the session's GitHub user is unknown`; a locked, missing or uninitialized vault yields
  its own message. `NoVaultsConfigured` through `for_subject` means exactly one thing: the
  serving daemon has no credential vaults wired (no `auth_storage`).
- The pre-n6 status path *was* token-keyed and could fail this way for token-less agent
  calls, but n6 (inbox + `for_subject`) removed that dependence — and link RPCs demonstrably
  work on the host in question, so it runs post-n6 code.

Verdict: the serving daemon (`LT-R1VXTH2V6H`) keeps no credential vaults, and the refresh
correctly surfaces that as `refresh_error` instead of hiding it. Expected when that daemon
never had an identity configured (a fresh Tddy Desktop, or auth never signed in there —
the Projects screen then reads "Accounts are not enabled on this host"); a separate bug
to file if accounts work there. No credential-design change made here, by instruction.

## Not done here

- The status stream's per-item check summaries (`item_check_summaries`,
  `work_queue_status_view`) still key off the assigned PR or the stored branch only: they
  take no sessions root, so a session-derived branch does not feed them. Same pre-existing
  behavior as for legacy-`session_id` items.
- `tddy-tools send` from this host to the cross-host orchestrator is refused as not
  addressable: the installed daemon predates cross-host messaging (n5, #1036). Progress
  reporting for this item goes through the session result instead.
- `boundChildSession` keeps its two legs (recorded child, branch owner): the fleet-wide
  `childSession` already takes precedence in the list and now covers linked children.

## Tests

- `tddy-changeset/tests/pr_stack_branch_resolution_acceptance.rs` — linked-session fallback,
  recorded-child precedence, unreadable links skipped in order, none readable resolves none.
- `tddy-daemon-rpc/tests/stack_plan_tools_acceptance.rs` — link a local and a remote-host
  session, status lists both with host and branch; a linked session's branch shows without
  being recorded.
- `tddy-tool-engine/src/work_queue_tools/tests.rs` — link parses its session and optional host.
- `tddy-workqueue-web/src/nodeChildSession.test.ts`, `hydrateWorkItems.test.ts`,
  `WorkItemLinkControls.test.ts` — the linked leg, linked hydration, the host label;
  `cypress/component/WorkItemLinksAcceptance.cy.tsx` — the row lists each link with its host
  (runs on CI; no Xvfb on this machine).

## Wrap

Wrapped 2026-11-25 by `/pr-wrap`. Verification was scoped to the touched packages:
`tddy-changeset` and `tddy-tool-engine` full suites green, `tddy-daemon-rpc`
`stack_plan_tools_acceptance` (15), `work_item_links_rpc_acceptance` (4) and
`project_home_session_tools_acceptance` (10) green, `cargo clippy -p` on the three with
`--all-targets -D warnings` clean, `cargo fmt --all --check` clean, and the
`tddy-workqueue-web` bun suite (164) plus `tsc --noEmit` green. The Cypress component spec
and the full workspace ride CI.

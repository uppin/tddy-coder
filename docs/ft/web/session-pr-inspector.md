# Session PR — the inspector's PR tab

**Component:** `SessionPullRequestTab` (`packages/tddy-workspace-web/src/pullRequests/`), mounted by
`SessionInspectorDrawer` (`packages/tddy-session-web/src/`)
**Status:** Implemented

## Overview

The session inspector's **PR** tab shows the pull request whose head is the session's branch:

- **Header**: `#<number> <title>`, linked to the PR on GitHub; its state (`open` / `draft` /
  `merged` / `closed`) and `head → base`.
- **Checks**: one row per check run at the PR's head. A queued check reads `Queued`. A running one
  reads `Running <elapsed>`, and the elapsed time ticks every second between updates. A completed
  one reads `<conclusion> in <duration>`. Each row links to the check's details page.
- **Comments**: reviews, review comments and conversation comments. A reply sits under the comment it
  answers, so a review thread reads as one unit. A reply whose parent is not in the view starts its
  own thread. A review shows its state (`APPROVED`, `CHANGES_REQUESTED`, …), and a review comment
  shows its file path.
- **Commits**: the PR's commits, oldest first, as short sha, subject and author.

A branch with no PR reads *This session's branch has no pull request.* The tab stays subscribed, so
the PR appears as soon as one is opened.

The tab only reads. It has no merge, no check re-run and no comment box.

## Live, through the daemon's GitHub inbox

No browser reads GitHub. The tab subscribes to `WorktreeService.StreamSessionPullRequest`. The
daemon answers with the current view, then sends a new one whenever its
[GitHub inbox](../../../packages/tddy-github-inbox/docs/architecture.md) reports a change to the PR row for the
session's branch, to the shown PR or its conversation or commits, or to the checks at its head.
Every read is an inbox row, so ten open inspectors of one PR make no more GitHub requests than one.

When the stream fails or ends, the tab shows the error beside the last view and subscribes again,
first after 1 s and then after twice the previous wait each time, up to 30 s. The error clears on the
first frame of the new subscription. An inbox read that fails while the stream is open, such as a
spent rate limit or an unreachable GitHub, arrives as a view carrying `unavailable_reason`, and the tab
shows that reason.

A **target session** (its agent on an ssh host or a Coder workspace) gets one view that carries only
`unavailable_reason`: the PR of a branch that lives on another host is not read from here yet
([backlog](../../dev/todo/2026-11-22-target-session-inspector-has-no-sync-state-or-pr.md)).

## The session list carries the PR

`SessionEntry.pr_number` and `SessionEntry.pr_state` hold the PR the inbox has **cached** for the
session's branch, or `0` and `""` when none is cached. Listing peeks at the cache and never waits on
GitHub. That cache is what lets the inspector pick its default tab without a second round trip and
without a tab that jumps after load.

## Default tab

With no `?inspector=` tab named, opening the inspector lands on:

1. **Work queue**, when the session owns a work queue (`SessionEntry.owns_work_queue`) — see
   [Session drawer § Work queue sidebar](session-drawer.md#work-queue-sidebar);
2. **PR**, when `SessionEntry.pr_number` is set;
3. otherwise **Worktree**.

An explicit `?inspector=` tab always wins, `details` included. The rule lives in
`defaultInspectorTab(session)` (`packages/tddy-session-web/src/defaultInspectorTab.ts`), and no rule
in it reads a session's role.

## Wire

```protobuf
service WorktreeService {
  rpc StreamSessionPullRequest(SessionWorktreeRequest) returns (stream SessionPullRequestView);
}
message SessionWorktreeRequest { string session_token = 1; string project_id = 2; string session_id = 3; }
message SessionPullRequestView {
  bool exists = 1;  uint64 number = 2;  string url = 3;  string title = 4;  string state = 5;
  string base_branch = 6;  string head_branch = 7;
  repeated PullRequestCheck checks = 8;  repeated PullRequestComment comments = 9;
  repeated PullRequestCommit commits = 10;  string unavailable_reason = 11;
}
```

## Reuse

`CheckRow`, `CommentThreads` (with `threadComments`) and `CommitList` are exported from
`tddy-workspace-web` for the other PR views: work-item rows and a PR's Inbox and Outbox.

## Related documentation

- [Session worktree inspector](session-worktree-inspector.md): the sibling Worktree tab and its sync
  state.
- [Session drawer § Inspector Tabs](session-drawer.md#inspector-tabs).
- [URL state routing](url-state-routing.md#params): the `inspector` param.
- Daemon: [worktree-service.md § The session PR view](../../../packages/tddy-worktree-service/docs/worktree-service.md#the-session-pr-view-pull_request_viewrs).

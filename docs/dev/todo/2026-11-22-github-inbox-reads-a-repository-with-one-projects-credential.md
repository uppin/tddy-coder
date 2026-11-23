# 2026-11-22 — The GitHub inbox reads a repository with the last-registered project's credential

**Category:** Design gap (left by `#workqueue` 2/14, #1003)
**Source:** `packages/tddy-session-lifecycle/src/connection_service/svc_trunk_watch_wiring.rs` (`HostRepoReaders`)
**Consent:** taken as the [W] worker's own recommendation under the stack's unattended-wrap consent (`/green-yolo` § Consent), 2026-11-22.

The inbox asks a `RepoReaders` port which transport reads a repository. The daemon's implementation keeps
**one reader per `owner/name`**, replaced each time a project subscribes its trunk, and never removes one.
So two projects of the same repository with different identities (two vault accounts, or a Local gh
project and a vault one) share whichever registered last, and a project that is deleted or re-pointed at
another account keeps reading through its old credential until another project registers.

Today the only reads are the trunk's commits, which every project of that repository can see anyway, so
nothing leaks. It stops being harmless when a second consumer reads through the inbox for a *caller* —
n4's PR tab, n6's work-item checks — since a caller without access to a private repository would be served
through someone else's token.

**Why deferred:** the trunk watch is the only consumer in #1003; the per-caller credential plumbing is n6's
(#1007) by the operator's decision, and choosing the rule (per-caller rows, or "any project of the caller")
belongs with it.

**Status after n6 (#1007, 2026-11-22): still open, and now live.** n6 made the caller-path reads real —
`GetPrStatus`/`QueryBranch`'s PR leg, `work_queue_status`, `AssignWorkItemPr` and `StreamWorkQueueStatus`
all read through the inbox via `SessionPullRequests::site_for_session`, which re-registers the *caller's*
project reader before each read. So the reader is the caller's at registration time, but (a) a concurrent
caller of the same repository can replace it between registration and read, and (b) the inbox's rows are
per repository, so a cached row first read with project A's token is served to project B without a request.
n6 did not close it because both halves are n2's surface and inside n6's `## Boundaries` ("does not change
the inbox, its rows or kinds"): `RepoReaders::reader_for` would take the caller's project, and the inbox
would need rows keyed by credential (or an access check per caller) — a change to the inbox's cache, not to
the queue RPCs. In practice a daemon's projects of one private repository belong to users who can all
read it, so the exposure needs two identities of differing access to one repository on one host.

**What would close it:** key readers by project as well as repository and have `reader_for` take the
caller's project; drop a project's reader when its last subscription ends; a test that two projects of one
repository with different accounts each read with their own.

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

**What would close it:** key readers by project as well as repository and have `reader_for` take the
caller's project; drop a project's reader when its last subscription ends; a test that two projects of one
repository with different accounts each read with their own.

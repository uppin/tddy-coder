# `restructure snapshot` writes the header a plan is missing - PRD

**Date**: 2026-10-05
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — the plan format (`## Plan format`) and the `snapshot` subcommand (the CLI table). A plan whose first line is an operation can now be given its header by `snapshot`.

No other feature document is affected. The warm-daemon `Snapshot` RPC described in
[Warm code intelligence daemon](../warm-code-intelligence-daemon.md) takes the same path and is unchanged in shape.

## Summary

`tddy-tools restructure snapshot <plan>` accepts a plan with no header line and writes one, computed from the files its
operations anchor. Every other command keeps refusing a headerless plan, and its refusal now says to run `snapshot`.

## Background

An author of a restructure plan emits an anchor with `restructure anchors`, wraps it in one operation line, and runs
`restructure check --deep`. It is refused: `plan is malformed: first line must be a snapshot header`. The command the
skill names for "rewrite the header", `restructure snapshot`, refuses the same file, because it only rewrites a header that
is already there. The author computes `sha256` by hand and writes a schema-v2 header, which is only hints for item anchors
and so is information the tool already has.

## Proposed Changes

### What's Changing

- `restructure snapshot <plan>` on a plan whose first non-blank line is an operation **inserts** line 1:
  - a **schema-v2** header (`{"v":2,"files":{"<path>":{"sha256":…,"modified":…}}}`) when every anchor of the plan is an `item` or `items` anchor;
  - a **schema-v1** snapshot header when any anchor is a `range` or a `symbol`, so drift is still refused for the anchors that depend on the whole file *(recommended; the alternative, v2 always, is an open decision recorded in the changeset)*;
  - naming exactly the files the operations' anchors name (every `also` anchor included, each file once);
  - leaving every operation line as the bytes it arrived as, and any leading blank lines.
- It is **idempotent**: a second `snapshot` of the plan it wrote changes nothing and reports `rewritten: false`.
- It refuses, writing nothing, an anchored file that is not there, an anchored file outside the workspace (absolute or `..`), a plan whose operations name no file, and any operation that does not parse (that operation's own refusal).
- `check`, `check --deep`, `apply`, `status` and `load` still refuse a headerless plan; the refusal ends *"this plan's first line is an operation, so run `restructure snapshot <plan>` to write one"*.
- A headerless plan's snapshot needs **no language server and no index daemon**, even when its anchors are item anchors: it never starts rust-analyzer and never dials `TDDY_INDEX_SOCKET`.

### What's Staying the Same

- The header formats (v1, v2) and what each promises.
- `snapshot` of a plan that has a header, byte for byte, including the re-resolution of item anchors on the warm index or a cold server.
- `restructure anchors` (it takes a file, not a plan), `verify`, `warm`, `apply`'s consumption of its plan, the plan store, every operation.
- The `Snapshot` RPC and the `tddy-tools` command line: no flag, no proto field, no subcommand is added.

## Impact Analysis

### Technical Impact

- `tddy-code-restructuring`: one new child module under `plan/codec/`, a few lines in `snapshot` and `snapshot_resolving`, one refusal text.
- `tddy-tools`, `tddy-index-daemon`: tests only.
- No dependency, no proto, no schema version change. File budget: `plan/codec.rs` stays <= 500 production lines.

### User Impact

- A plan author runs `anchors` -> write one operation line -> `snapshot` -> `check --deep` -> `apply`, with no hand-computed hash.
- A plan author who ran `check` first is told what to do, instead of the same text `snapshot` also refuses with.
- No breaking change: a plan that had a header behaves exactly as before.

## Implementation Plan

1. Library: the tolerant read, the header from anchored files, the insert-splice in `snapshot` (tests first, then the code).
2. `snapshot_resolving` reaches `snapshot` for a headerless plan.
3. The refusal text of the other readers.
4. Pin the routing: CLI with and without a named socket, and the daemon's `Snapshot` coordinate over fake language servers.
5. Docs at wrap: the plan-format section of the feature doc, `plan-schema.md`, SKILL.md step 6, the package README line.

## Acceptance Criteria

- [ ] `snapshot` of a one-operation headerless plan writes a header naming exactly the anchored files, and the operation line is unchanged ([Rust code restructuring](../rust-code-restructuring.md))
- [ ] `snapshot` twice changes nothing the second time
- [ ] a missing or out-of-workspace anchored file refuses and writes nothing
- [ ] `check` of a headerless plan is refused naming `restructure snapshot`
- [ ] the CLI, with `TDDY_INDEX_SOCKET` set to an unreachable socket, still writes the header (no dial), and the daemon's `Snapshot` holds no root for it
- [ ] a plan that anchors by `range` gets a header that still refuses drift (if the recommended decision stands)
- [ ] tests pass for `tddy-code-restructuring`, `tddy-tools` and `tddy-index-daemon` (scoped; CI for the rest)

## References

### Affected Features (Complete List)

- [Rust code restructuring](../rust-code-restructuring.md) — plan format, `snapshot`

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-10-05-sharpen-plan-header.md`
- Todo this closes (exists only on `feature/carve/lifecycle-ports-agents`): `docs/dev/todo/2026-10-05-restructure-snapshot-cannot-write-a-missing-plan-header.md`
- Schema: `.agents/skills/code-restructuring/references/plan-schema.md` (dev-only)

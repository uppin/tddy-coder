# An orchestrator writes session files and attaches them to the sessions it starts - PRD

**Date**: 2026-11-21
**PRD Type**: Enhancement
**Stack**: `#orchestra` 3/9 — branch `feature/orchestra/session-files`, base `feature/orchestra/child-back-channel`

## Affected Features

- **Primary Feature**: [Project concept — orchestrator sessions](../project-concept.md)

## Summary

An orchestrator can write files into its **own** session directory with `WriteSessionFile` (a brief, a plan, a spec) and list them with `ListSessionFiles`. `StartSession` takes such a file as an attachment by name; the daemon copies it into the child's attachments the same way an uploaded file is.

## Background

`StartSession` accepts attachments by reference to existing documents, but nothing lets an agent create one, and an orchestrator cannot write at all (`role_guard.rs:44-52`).

## Proposed Changes

### What's Changing
- `WriteSessionFile { name, content }` writes `{session_dir}/artifacts/files/<name>` (flat; no `/`, no `..`; size-bounded).
- `ListSessionFiles` lists them with size and mtime.
- `StartSession.attachments[]` gains a `session_file` source naming one of the caller's files.

### What's Staying the Same
- The read-only rule for everything else; this is a write into the session's own directory, not into any worktree.
- Browser staging and existing attachment scopes.

## Impact Analysis

### Technical Impact
- **tddy-session-files**: writer for an orchestrator's own session files (`artifacts/files/<name>`), name validation
- **tddy-tool-engine**: `WriteSessionFile` / `ListSessionFiles` tools; `StartSession.attachments` accepts a session-file reference
- **tddy-daemon-rpc**: role guard admits the two tools for orchestrator roles

### User Impact
An orchestrator hands a child a written brief instead of cramming it into the first message.

## Implementation Plan

One node: `feature/orchestra/session-files` (#orchestra 3/9).

## Acceptance Criteria

- [ ] An orchestrator writes a session file and lists it.
- [ ] A name with a path separator or `..` is refused.
- [ ] A started session receives the file as an attachment.
- [ ] A worker session is refused `WriteSessionFile`.

# The orchestrator is told to run all coding through tracked tddy sessions - PRD

**Date**: 2026-11-21
**PRD Type**: Enhancement
**Stack**: `#orchestra` 9/9 — branch `feature/orchestra/orchestrator-prompt`, base `feature/orchestra/orchestrator-lsp`

## Affected Features

- **Primary Feature**: [Project concept — orchestrator sessions](../project-concept.md)

## Summary

The role prompt tells an orchestrator: do all coding work by opening tddy sessions; track each one (watches, transcript, activity, messages) until it ends; tell every child to report progress and status periodically over the back channel its guidance names; reach children by their listed route; check and refresh the base before starting work and react to trunk-commit activities; read beyond defaults and write only through `RequestPathAccess`; use the LSP tools and roots; release terminals it claimed. Main and orchestrator sessions get their own opening line.

## Background

`project_home_prompt.md` (44 lines) says "You are this project's main session" to every orchestrator role and omits watches, messages, turn control, lifecycle and stack tools.

## Proposed Changes

### What's Changing
- One prompt, two openings (main / orchestrator).
- Sections: coding goes through sessions; tracking discipline; instructing children to report; delivery routes; base freshness and trunk activities; reach and access requests; session files; LSP; terminal claim and release.
- Every tool the catalogue offers an orchestrator is named in the prompt.

### What's Staying the Same
- How the prompt is delivered (`--append-system-prompt-file`).

## Impact Analysis

### Technical Impact
- **tddy-agent-launch**: `project_home_prompt.md` rewritten; role-specific opening for main vs orchestrator

### User Impact
The orchestrator behaves like one: it delegates coding, follows its children, and keeps its base current.

## Implementation Plan

One node: `feature/orchestra/orchestrator-prompt` (#orchestra 9/9).

## Acceptance Criteria

- [ ] An orchestrator's prompt opens as an orchestrator; main's as main.
- [ ] The prompt tells it to start sessions for coding and track them.
- [ ] The prompt tells it to instruct children to report periodically over their back channel.
- [ ] Every tool in the orchestrator catalogue is named in the prompt.

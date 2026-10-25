# Snapshots for jailed terminals - PRD

**Date**: 2026-10-25
**PRD Type**: Enhancement
**Stack**: `#term-snapshot 3/4` (branch `feature/term-snapshot/jail-size`)

## Affected Features

- **Primary**: [Terminal sessions (daemon)](../terminal-sessions.md) - sandboxed terminals answer `SNAPSHOT`
- **Related**: sandboxed sessions (`tddy-sandbox-runner`, `tddy-daemon-sandbox`; no feature doc of their own) - the runner reports its PTY size to the daemon

## Summary

A sandboxed session's terminal gets the same snapshot-on-(re)connect as every other terminal. To make that possible, the runner inside the jail reports its PTY size (at spawn and on every resize) over the stdio bridge, so the daemon's screen model tracks the jail's PTY.

## Background

The daemon's adapter for a jailed terminal cannot resize, and the runner resizes inside the jail, so the daemon never learns the size. A screen model at the wrong size produces a wrong snapshot. `#term-snapshot 1/4` therefore refuses `SNAPSHOT` for jailed terminals with `FAILED_PRECONDITION`.

## Proposed Changes
### What's Changing
- The runner reports the PTY size over the stdio bridge, at spawn and after each resize.
- The daemon resizes the terminal's screen model to the reported size, at the same sequence point as the output stream.
- Jailed terminals answer `SNAPSHOT` like any other terminal; the `FAILED_PRECONDITION` refusal is removed.
### What's Staying the Same
- Who resizes a jailed PTY, and the jailed terminal's other behavior.

## Acceptance Criteria
- [ ] After the runner spawns at R×C and later resizes to R'×C', a `SNAPSHOT` open on the daemon yields a snapshot serialized at R'×C' that renders identically to the jail's screen.
- [ ] A jailed terminal no longer answers `SNAPSHOT` with `FAILED_PRECONDITION`.

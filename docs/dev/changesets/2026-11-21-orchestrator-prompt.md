# 2026-11-21 — The orchestrator is told to run all coding through tracked tddy sessions

**Type:** Feature · **Stack:** `#orchestra` 9/9 (PR #987) · **Packages:** `tddy-agent-launch` ·
Feature: [project-concept.md § Role prompt](../../ft/daemon/project-concept.md)

## Problem

`project_home_prompt.md` said "You are this project's main session" to every orchestrator-role
session and omitted watches, messages, turn control, lifecycle, stack, LSP and access tools.

## What changed

- **`tddy-agent-launch::project_home_launch`**: `project_home_prompt(role)` returns the shared body
  under a role-specific opening ("this project's main session" for `project-home`, "one of this
  project's orchestrators" for `orchestrator`). `ProjectHomeLaunch` and `MainLaunchPlan` carry the
  session's `role`, and `project_home_wiring` writes `project_home_prompt(role)`.
- **`project_home_prompt.md`**: a "How you work" section (coding through sessions it starts and
  follows; children asked to report progress and status over their back channel; delivery routes;
  base freshness and `trunk_commit` activities; `RequestPathAccess`; session files; LSP roots;
  releasing terminals) and a "Your tools" reference that names every tool the catalogue offers.

## Decisions

- **One body, two openings**, not two prompts: the roles differ in who they are, not in what they may
  do, and a second file would drift from the catalogue.
- **A test pins the catalogue**: `every_tool_the_orchestrator_is_offered_is_named_in_its_prompt`
  fails when a tool is added to `project_home_offered_catalog` without a line in the prompt.

## Tests

`packages/tddy-agent-launch/src/project_home_launch/project_home_prompt_tests.rs` (openings, coding
through sessions, back-channel reporting, base freshness, catalogue coverage).

# Coder — Product Area Overview

**Type**: Technical Product (Developer Tool)
**Status**: Active
**Updated**: 2026-03-10

## Summary

tddy-coder is a TDD-driven development CLI that orchestrates an LLM backend (Claude Code or Cursor) through a strict workflow: plan → acceptance-tests → red → green → demo → evaluate → validate → refactor → update-docs. It produces structured artifacts (PRD.md, TODO.md, acceptance-tests.md, progress.md, etc.) in a plan directory and maintains workflow state in changeset.yaml. The tool supports both TUI mode (interactive ratatui interface) and plain mode (linear output for piping and scripting).

## Target Users

- **Developers** using TDD to build features from a natural-language description
- **Teams** adopting structured planning and acceptance-test-driven workflows
- **Automation** via piping, `--prompt`, and gRPC remote control

## Core Capabilities

| Capability | Description |
|------------|--------------|
| **Planning** | Accepts feature description via stdin or `--prompt`; invokes LLM in plan mode; produces PRD.md, TODO.md, changeset.yaml |
| **Plan Approval** | After plan completes, user can View PRD, Approve (proceed), or Refine (feedback loop) |
| **Acceptance Tests** | Creates failing acceptance tests from PRD; writes acceptance-tests.md |
| **Red-Green** | Red creates skeletons and failing tests; Green implements production code to make them pass |
| **Demo** | Executes demo plan from demo-plan.md (optional, prompted after green) |
| **Evaluate** | Analyzes git changes for risks; produces evaluation-report.md |
| **Validate** | Subagent-driven validation (tests, prod-ready, clean code) |
| **Refactor** | Executes refactoring plan from validate phase |
| **Update Docs** | Reads planning artifacts and updates target repo documentation per repo guidelines |
| **TUI** | Full ratatui interface: activity log, status bar, inbox, clarification prompts, plan approval |
| **gRPC** | `--grpc` exposes bidirectional streaming for programmatic control (E2E tests, automation); `StreamTerminal` streams raw TUI bytes for remote viewing |

## Feature Documents

| Feature | Description |
|---------|-------------|
| [Planning Step](planning-step.md) | Plan goal, acceptance-tests goal, plan approval gate, CLI interface, LLM backend abstraction |
| [Implementation Step](implementation-step.md) | Red, green, demo, evaluate goals; state machine; output artifacts |
| [gRPC Remote Control](grpc-remote-control.md) | `--grpc` flag, bidirectional streaming, programmatic control for E2E and automation |

## Integration Points

- **tddy-core**: Workflow engine, graph-flow-compatible tasks, RunnerHooks, CodingBackend trait
- **tddy-tui**: Ratatui view layer, PresenterView implementation, key mapping
- **tddy-grpc**: gRPC service, proto definitions, event conversion
- **tddy-demo**: Same app with StubBackend for demos and E2E tests
- **Claude Code CLI / Cursor**: LLM backends invoked via subprocess/API

## Change History

See [changelog.md](changelog.md) for release note history.

## Appendices

Technical specifications and supporting documentation:

- **[Technology Stack](../../dev/guides/tech-stack.md)** — Core technologies, integration patterns
- **[Testing Practices](../../dev/guides/testing.md)** — Anti-patterns, unit/integration/production test guidelines

# 2026-03-09 — Async Workflow Engine with Graph-Flow-Compatible Traits

- **CodingBackend**: Trait is now async; all backends (Claude, Cursor, Mock, Stub) use async invoke.
- **Graph-flow modules**: Task, Context, Graph, FlowRunner, SessionStorage in tddy-core. PlanTask writes PRD.md and TODO.md; BackendInvokeTask for other steps. `build_tdd_workflow_graph()` defines plan→acceptance-tests→red→green→end topology.
- **StubBackend**: New backend for demo and workflow tests. Magic catch-words: CLARIFY, FAIL_PARSE, FAIL_INVOKE. Returns schema-valid structured responses.
- **tddy-demo**: New package — same app as tddy-coder with StubBackend. `--agent stub` only. Self-documenting tutorial.
- **run_plan_via_flow_runner**: FlowRunner-based plan execution; used when migrating CLI/TUI from Workflow to FlowRunner.
- **Backend create-once**: SharedBackend wraps backend; created once per run, reused across goals.

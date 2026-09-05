# 2026-03-09 — Async Workflow Engine with Graph-Flow-Compatible Traits

**Type:** Feature

`CodingBackend::invoke()` async. New workflow modules: Task, Context, Graph, GraphBuilder, Session, SessionStorage, FlowRunner. PlanTask writes PRD.md/TODO.md; BackendInvokeTask for acceptance-tests, red, green. `build_tdd_workflow_graph()` builds plan→acceptance-tests→red→green→end. StubBackend with magic catch-words (CLARIFY, FAIL_PARSE, FAIL_INVOKE). SharedBackend wraps Arc<dyn CodingBackend>; backend created once per run. tddy-demo package: StubBackend + TUI. Dependencies: tokio, dashmap, async-trait. (tddy-core)

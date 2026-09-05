# 2026-03-09 — MVP Architecture

**Type:** Feature

Presenter module in tddy-core: UserIntent, PresenterState, PresenterView trait, Presenter, workflow_runner. Presenter owns app state and workflow orchestration; receives UserIntents only. Workflow thread sends WorkflowEvent (GoalStarted, ClarificationNeeded, WorkflowComplete, etc.). Inbox dequeue restarts workflow with prefixed prompt. StubBackend: recognize "HERE ARE THE USER'S ANSWERS" for clarification follow-up. (tddy-core)

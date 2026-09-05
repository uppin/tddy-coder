# 2026-03-08 — TDD Workflow Restructure

**Type:** Feature

Demo goal: Goal::Demo, workflow.demo(), DemoOptions, DemoRunning/DemoComplete/DemoSkipped states. GreenOptions.run_demo removed. next_goal_for_state: GreenComplete → demo, DemoComplete/DemoSkipped → evaluate. Early changeset: plan() writes minimal changeset.yaml before backend invoke. evaluate() accepts GreenComplete/DemoComplete/DemoSkipped. (tddy-core)

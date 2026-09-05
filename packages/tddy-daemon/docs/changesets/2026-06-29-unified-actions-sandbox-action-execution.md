# 2026-06-29 — Unified actions + sandbox action execution

**Type:** Feature

`ActionServiceImpl` (`ListActionKinds`/`StartAction`/`GetAction`); `sandbox_plan_builder` + `sandbox_action` (confined process + runner-PTY via `tddy-sandbox-runner --pty-command`); `PtyRuntime`/`ProcessRuntime` integration; fast tools + session jobs + PTY terminals share `TaskRegistry`; acceptance: `action_sandbox_acceptance`, `action_service_acceptance`. Feature [background-tasks.md](../../../../docs/ft/daemon/background-tasks.md). PR [#244](https://github.com/uppin/tddy-coder/pull/244). (tddy-daemon, tddy-actions, tddy-sandbox-recipes, tddy-sandbox-runner)

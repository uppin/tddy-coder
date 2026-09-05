# 2026-06-29 — **Action sandbox plans

**Type:** Feature

cwd + extra_read_paths** — `SandboxBuilder::cwd()`, `SandboxSpec.cwd`, `extra_read_paths` plumbed through darwin/cgroups `spawn_plan`; daemon `sandbox_plan_builder` maps `ActionSpec` sandbox metadata to process or `RunnerPty` plans. Feature [sandbox-builder.md](../../../../docs/ft/coder/sandbox-builder.md). (tddy-sandbox, tddy-sandbox-darwin, tddy-sandbox-cgroups, tddy-sandbox-recipes, tddy-daemon)

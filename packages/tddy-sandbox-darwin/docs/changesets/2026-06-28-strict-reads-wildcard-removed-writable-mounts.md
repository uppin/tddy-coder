# 2026-06-28 — Strict reads (wildcard removed) + writable mounts

**Type:** Feature

`render_plan(&SandboxPlan)` emits an explicit read allow-list with **no `(allow file-read*)`** (dyld root `/` + system libs + `/dev/*` nodes + ICU + toolchain + `claude` `otool -L` deps); deleted the SBPL template, `render_profile`, and the old spec-based `spawn`; `spawn_plan` materializes copies/symlinks/secrets; `MountSpec` grants read/write(+exec) for host dirs at their real path; OAuth secret kept out of `sandbox-exec` argv. Acceptance: strict `claude --version` boot, out-of-tree read denied, shell reads `/dev/null`, secret-not-in-argv, writable-mount render. (tddy-sandbox-darwin, tddy-sandbox)

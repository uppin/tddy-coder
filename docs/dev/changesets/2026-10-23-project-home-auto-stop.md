# 2026-10-23 — Project-home sessions stop once idle (#project-home 6/7)

**PR:** #701 · **Packages:** tddy-daemon-kernel, tddy-agent-launch, tddy-session-lifecycle, tddy-daemon

A daemon task stops a running project-home session once its agent has been idle for `project_home.idle_stop_secs` (default 600, `0` = off); `EnsureProjectHomeSession` resumes the same conversation. Feature: [project-home auto-stop](../../ft/daemon/project-home-auto-stop.md). Package entries: [tddy-agent-launch](../../../packages/tddy-agent-launch/docs/changesets/2026-10-23-project-home-idle-reaper.md), [tddy-daemon-kernel](../../../packages/tddy-daemon-kernel/docs/changesets/2026-10-23-project-home-config.md), [tddy-session-lifecycle](../../../packages/tddy-session-lifecycle/docs/changesets/2026-10-23-project-home-reap-entry-point.md), [tddy-daemon](../../../packages/tddy-daemon/docs/changesets/2026-10-23-project-home-reaper-task.md).

Decisions: idle = after turn with no input (not "left the view" — the daemon cannot see the view); a 30 s periodic task rather than per-session timers, so state survives daemon restarts. Idleness comes from the hook-reported `activity_status`/`updated_at`, not the inferred `SessionAgentStatus` (`UNSPECIFIED` for sessions without an observed conversation).

Validation at wrap: stack gate — on n5's tip `ee4cfc65b`, `origin/<base>..HEAD` this PR's commits only. Cleanups — `unsafe kill(pid, 0)` carries a SAFETY comment; the config read no longer clones; the 30 s period is a named constant; the acceptance stub now stays alive (`exec cat`) like an agent waiting for input, so the reaper's `is_active` precondition no longer races the stub's exit (the CI risk [G] recorded). File-length gate: no non-test file this PR touched is at or over 500 production lines (largest `runtime/tasks.rs` 312 → 341). Not split (stack-shared with other #project-home PRs, none over budget): `tddy-daemon` `runtime/tasks.rs`, `mount_host.rs`, `tddy-daemon-kernel` `config.rs`/`config/sections.rs`. Code issues: none added — new functions measured by line count and nesting (every one ≤ 40 lines, nesting ≤ 3; no instrumented/coverage build).

Carried to the stack cascade: `project_home_idle_stop_acceptance.rs` passes `None` as `DaemonSessionHost::new`'s 7th argument, correct on this base; once the base contains master's #704 it must become `Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel)`.

Opened: backlog `2026-10-23-project-home-auto-stop-untracked-processes.md` (an alive but untracked session process is not stopped).

Verified (scoped, build only per developer instruction 2026-10-23): `cargo clippy -p tddy-agent-launch -p tddy-daemon-kernel -p tddy-daemon --all-targets -- -D warnings`, `cargo check -p tddy-session-lifecycle --all-targets`, `cargo fmt`. Tests: deferred to CI.

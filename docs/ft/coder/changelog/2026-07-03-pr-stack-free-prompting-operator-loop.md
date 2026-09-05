# 2026-07-03 — PR-stack: free-prompting operator loop

- The `pr-stack` orchestrator no longer runs an automatic agentic loop; after planning it drops into an interactive `orchestrate` free-prompting chat where the developer drives the stack turn-by-turn.
- The agent manages the stack through new `tddy-tools` tools: list/status, merge to master, repoint, close, resolve conflicts, set status, add planned PR, and spawn a child session — replacing the autopilot.
- Each planned PR gains an internal status (`needs-repoint`/`has-conflicts`/`ready-to-merge`/`merged`/`blocked`/`up-to-date`), auto-derived from git+GitHub but overridable by the agent, shown as a colored badge on planned-PR rows.
- `pr_spawn_child` spawns a stack child session from chat via a daemon relay (⚠️ end-to-end spawn needs live-daemon verification before relying on it; the web "Start session" button remains available).

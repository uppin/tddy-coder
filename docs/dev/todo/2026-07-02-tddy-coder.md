# 2026-07-02 — tddy-coder

**Category:** Future enhancement
**Source:** subagent-tool-replacement changeset, 2026-07-02

- **Extend subagent tool-replacement to the `--remote` path** — `packages/tddy-coder/src/remote.rs`'s `RemoteContextDir`/`REMOTE_APPENDIX` and `build_remote_allowlist` have no subagent concept at all today (no `SUBAGENT_TOOLS`, no `subagent_*` wiring in `run_remote`). Extending the tool-replacement mechanism there requires wiring subagent support into that path first — deferred to keep this changeset scoped to the sandbox/daemon managed path where subagents already exist.
- **Per-tool replacement policies** — the replaced-tool set is a flat list today (e.g. all of `Grep`, unconditionally). A future refinement could scope replacement (e.g. only for certain file types or path prefixes) rather than an all-or-nothing per-tool switch.

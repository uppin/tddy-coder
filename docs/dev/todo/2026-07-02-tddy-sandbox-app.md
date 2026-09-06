# 2026-07-02 — tddy-sandbox-app

**Category:** Future enhancement
**Source:** specialized-subagents changeset, 2026-07-02

- ~~`--specialized-agent` CLI flag + deprecated aliases~~ — done (2026-07-02, multi-agent tool-replacement changeset; overrides and the deprecated alias fully removed 2026-07-02 in a follow-up cleanup — see below). `tddy-sandbox-app` takes repeatable `--specialized-agent <name>` + `--agents-dir`, resolves them via `spawn::resolve_specialized_agents`, and threads the resolved array into the jail as `TDDY_SUBAGENT`/`TDDY_SUBAGENTS_JSON` via `spawn::subagent_env_overlay`. There is no `--discovery-subagent` alias and no `--fastcontext-*`/`--subagent-replaces` override flags — all configuration comes exclusively from the resolved agent's YAML def. See `docs/ft/coder/managed-codebase-subagents.md` and `docs/ft/coder/specialized-subagents.md`.
- ~~**`--agent` CLI validation for custom specialized-agent names (`tddy-coder`)**~~ — **done** (models-and-assistants changeset, 2026-08-16). The clap `value_parser` allowlist on `--agent` is removed; validation moved into `create_backend`, whose former `_ => Claude` catch-all is now an error naming the known agents. A registry assistant or a `<tddyhome>/agents` def is accepted by name.

# 2026-07-21 — tddy-workflow-recipes / tddy-discovery

**Category:** Future enhancement
**Source:** exploration-artifact changeset, 2026-07-21

- **Prime `exploration.md` from the FastContext discovery subagent** — the discovery agent already returns `path:line-start-line-end` citations (`docs/ft/coder/discovery-agent.md`); seed the exploration artifact from those citations before the plan agent starts so plan-time exploration begins pre-warmed.
- **Structured exploration entries in `changeset.yaml` discovery** — extend `DiscoveryData.relevant_code` with line/col-aware references sourced from the exploration document, keeping a machine-readable mirror of the markdown.
- **Staleness detection for exploration line references** — flag `exploration.md` code references invalidated by later diffs (e.g. compare against `git diff` ranges in post-green steps) so downstream agents know which references to re-verify.

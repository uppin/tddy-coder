# tddy-agent-skills

Project agent skills (`.agents/skills/<folder>/SKILL.md`): discovery, validation, slash-menu entries and prompt composition, plus the `/start-<recipe>` feature-prompt contract.

## Quick Start

```bash
cargo build -p tddy-agent-skills
cargo test -p tddy-agent-skills
```

## Dependencies

No workspace dependencies — a leaf.

It never depends on `tddy-core`. `packages/tddy-core/tests/core_facade_shape.rs` pins the
dependency order of every crate carved out of `tddy-core` and holds each at or under 10,000
production lines.

## Module layout

| Module | Owns |
|---|---|
| `agent_skills` | `scan_skills_at_project_root`, `slash_menu_entries`, `compose_prompt_with_selected_skill` |
| `feature_start_slash` | the `/start-<recipe>` commands and the default free-prompting recipe name |

## Relationship to `tddy-core`

This code lived in `tddy-core`, which re-exports this crate whole (`pub use tddy_agent_skills::*;`), so
every `tddy_core::…` path it provides still resolves. Write new code against
`tddy_agent_skills` directly.

## Documentation

- [Architecture](docs/architecture.md)
- [Changesets](docs/changesets/) — applied changeset history

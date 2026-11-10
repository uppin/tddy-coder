# 2026-11-10 — codex_acp_route_args is toml_route_overrides

**Type:** Refactor

`#agent-tools` 33/33 ([#876](https://github.com/uppin/tddy-coder/pull/876)).

- The route renderer is named for the shape it renders, `toml_route_overrides` (re-exported as
  `tddy_core::toml_route_overrides`), not for one agent. The session-lifecycle launcher calls it for
  any spec whose `route.shape` is `toml_args`, and `launcher_names_no_agent` pins that the launcher
  names no shipped agent.

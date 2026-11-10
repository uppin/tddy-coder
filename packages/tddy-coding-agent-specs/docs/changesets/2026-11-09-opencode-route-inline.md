# 2026-11-09 — OpenCode declares its route inline (#agent-tools 29/32, #857)

- `routed_agent` copies `route.config_env` into `RoutedAgent.config_env`.
- `route_slot_errors` (called from `parse_section`) refuses an `inline_config` slot without `config_env`, with a `TDDY_*` one, or with a `files_dir`.
- `specs/opencode.yaml`: `route.shape: inline_config`, `config_env: OPENCODE_CONFIG_CONTENT`, no `files_dir`, no `acp.hooks_adapter`.
- `src/spec.rs` 1110 → 1152 production lines (stack-shared; sweep node).

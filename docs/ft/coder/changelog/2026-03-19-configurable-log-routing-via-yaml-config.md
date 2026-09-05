# 2026-03-19 — Configurable Log Routing via YAML Config

- **Log config section**: YAML `log:` section with named loggers (output target + format) and policies that reference loggers by name. Selectors: target (exact/glob), module_path, heuristic. First-match-wins ordering.
- **CLI**: `--log-level <level>` overrides default policy level. Removed `--debug`, `--debug-output`, `--webrtc-debug-output`.
- **Log rotation**: On startup, existing log files renamed with timestamp suffix; rotated files beyond `max_rotated` pruned.
- **Packages**: tddy-core (log_backend.rs), tddy-coder (config.rs, run.rs).

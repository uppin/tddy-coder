# 2026-03-19 — Configurable Log Routing via YAML Config

**Type:** Feature

LogConfig with named loggers (output target + format) and policies that reference loggers by name. Selectors: target (exact/glob), module_path, heuristic. TddyLogger routes records to first-matching policy. Log rotation on startup (timestamp rename, max_rotated retention). init_tddy_logger(config: LogConfig). (tddy-core)

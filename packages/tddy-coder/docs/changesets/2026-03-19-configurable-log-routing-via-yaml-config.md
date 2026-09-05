# 2026-03-19 — Configurable Log Routing via YAML Config

**Type:** Feature

Removed `--debug`, `--debug-output`, `--webrtc-debug-output`. Added `log: Option<LogConfig>` to Config, `--log-level` CLI override. init_tddy_logger receives LogConfig. config.example.yaml documents log section. (tddy-coder)

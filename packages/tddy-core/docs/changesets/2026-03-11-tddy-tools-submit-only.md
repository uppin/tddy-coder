# 2026-03-11 — tddy-tools Submit Only

**Type:** Feature

Removed all inline parsing (XML, delimiters) from output/parser.rs. Parser functions accept JSON-only from tddy-tools submit. Fail-fast when no submit result. Stream parsing no longer extracts structured-response blocks. verify_tddy_tools_available at tddy-coder startup. (tddy-core)

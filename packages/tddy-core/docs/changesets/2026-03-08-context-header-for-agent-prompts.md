# 2026-03-08 — Context Header for Agent Prompts

**Type:** Feature

build_context_header and prepend_context_header prepend `<context-reminder>` block with **CRITICAL FOR CONTEXT AND SUMMARY** and absolute paths to existing .md artifacts (PRD.md, TODO.md, acceptance-tests.md, etc.) to plan, acceptance-tests, and red prompts. Omitted when no artifacts exist. (tddy-core)

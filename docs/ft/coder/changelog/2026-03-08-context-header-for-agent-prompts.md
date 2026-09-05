# 2026-03-08 — Context Header for Agent Prompts

- **Context reminder**: Plan, acceptance-tests, and red prompts are prepended with a `<context-reminder>` block listing absolute paths to existing .md artifacts (PRD.md, TODO.md, acceptance-tests.md, etc.) when the session directory contains them.
- **Format**: Header starts with `**CRITICAL FOR CONTEXT AND SUMMARY**`; each line is `{filename}: {absolute_path}`. Omitted when plan dir is empty or no .md files exist.
- **Agent awareness**: Agents receive immediate visibility of available plan context files without discovering them.

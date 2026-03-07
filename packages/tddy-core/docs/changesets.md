# Changesets Applied

Wrapped changeset history for tddy-core.

- **2026-03-07** [Feature] Permission Handling in Claude Code Print Mode — Extended InvokeRequest with allowed_tools, permission_prompt_tool, mcp_config_path, working_dir, debug. Added permission module (plan_allowlist, acceptance_tests_allowlist). build_claude_args passes --allowedTools, --permission-prompt-tool, --mcp-config. Stream parsing extracts structured output from user tool_result (Claude CLI bug workaround). (tddy-core)
- **2026-03-07** [Feature] Acceptance Tests Goal — Added acceptance_tests workflow, AcceptanceTesting/AcceptanceTestsReady states, session persistence (.session), parse_acceptance_tests_response, PermissionMode::AcceptEdits for acceptance-tests. (tddy-core)
- **2026-03-07** [Feature] Claude Stream-JSON Backend — Replaced plain-text backend with NDJSON stream processing (`--output-format=stream-json`), session management (`--session-id`/`--resume`), structured question extraction from AskUserQuestion, structured-response and delimited PRD/TODO parsing, progress callback. (tddy-core)
- **2026-03-06** [Feature] Planning Step Implementation — Added CodingBackend trait, ClaudeCodeBackend, MockBackend, Workflow state machine, output parser, artifact writer. (tddy-core)

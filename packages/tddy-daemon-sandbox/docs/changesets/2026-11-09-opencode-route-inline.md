# 2026-11-09 — A jail reaches a base URL inside an inline route value (#agent-tools 29/32, #857)

- `tool_jail::route_hosts` reads every `http(s)` URL inside each route variable's value, so OpenCode's inline `OPENCODE_CONFIG_CONTENT` admits its endpoint.

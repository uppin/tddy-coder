# 2026-08-30 — `SessionFrame` carries a tool call host → jail

**Type:** Feature

the channel carried `tool_request`/`tool_response` in one direction only, the in-jail agent asking the host to run a tool for it. A workspace tool jail hosts no agent and needs the reverse, so the oneof gains `in_jail_tool_request = 16` (host → jail) and `in_jail_tool_response = 17` (jail → host). The response carries no request id: the host keeps one in-jail call outstanding at a time, so the answer belongs to the request its sender holds the lock for. (tddy-service)

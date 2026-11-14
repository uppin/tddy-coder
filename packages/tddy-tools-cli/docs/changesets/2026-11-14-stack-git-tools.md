# 2026-11-14 — The advertised MCP surface pins 34 names

**Type:** Feature

The advertisement audit is n7's 40/37 surface minus the six static `pr_*` git tools that are
the daemon's exec tools now: exactly 34 tool names where the host serves the action tools and
31 where it does not, differing in exactly those three. The count is recomputed from the
test's own array, which the compiler checks (`[&str; 34]`).

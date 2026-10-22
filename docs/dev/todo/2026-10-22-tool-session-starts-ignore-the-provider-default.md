# 2026-10-22 — a tool session that names no account does not use the person's default

**Category:** Deferred from `#provider-route` 5/9 `default-route` (#666)
**Source:** `/validate-changes` on #666

#666 feeds `RouteSources.user_default` (`session_route_sources_with_default` in
`packages/tddy-agent-launch/src/session_provider_route.rs`) into the claude-cli and opencode-cli
starts, and into the sandboxed claude-cli start's refusal. A **tool session** still decides its route
from the session-level choice alone, so a person whose default for `codex` is Meta gets the vendor
on a tool session that names no account.

**Why deferred:** the tool-session start is #665's (`#provider-route` 4/9, `tool-route`). When #666
was wrapped, #665's `tool_session_route_json` was still a `todo!()` and no tool-session start called
it, so #666 had no call site to feed.

**Plan pointer:** once #665 is green and #666 is rebased onto it, change the tool-session start's
`session_route_sources(req)` to `session_route_sources_with_default(req, <the agent's harness id>,
&access)`. That is a one-line change plus a test in `tool_session_route_tests.rs`: a tool session
that names no account is handed the default's route file. It belongs to whichever of #665 or #666
lands second, or to the stack's sweep node.

## What would close it

A tool session started with no account choice resolves its route through `user_default`, pinned by
a test, and this entry is deleted.

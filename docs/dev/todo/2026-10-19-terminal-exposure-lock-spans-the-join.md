# 2026-10-19 — The terminal-exposure lock spans the join

**Category:** Known limitation
**Source:** `#satellite` 6/13 (#628), PRD out of scope

`CliSessionManager::ensure_exposed_terminal` holds the registry's write lock across `TerminalExposure::expose`,
which for LiveKit is a token mint and a room join. That is what makes it single-flight (two reaches at once would
otherwise evict each other under one identity), but it also means one session's join blocks every other session's
join and every other registry access for as long as the join takes. Still true after the move behind the port.

**What would close it:** a per-session in-flight marker (or a per-session lock) in the record, so only reaches to the
same session wait on each other. Needs a test that two different sessions join concurrently.

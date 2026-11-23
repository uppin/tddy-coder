# 2026-11-22 — The trunk watch reads GitHub through one shared, conditional inbox

- The daemon keeps one GitHub inbox: a conditional feed per repository and a cache of PR, check, comment and commit rows, each revalidated with its ETag, so reads cost a fixed amount per repository rather than per reader.
- Orchestrators' trunk watch reads through it; projects sharing a repository share its reads. Activities and who is subscribed are unchanged.
- When the GitHub quota runs low the feed waits for the reset and reads answer from the cache.

Feature: [project-concept.md § Watching the trunk](../project-concept.md#watching-the-trunk)

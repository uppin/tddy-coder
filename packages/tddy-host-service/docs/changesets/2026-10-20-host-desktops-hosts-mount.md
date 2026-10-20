# 2026-10-20 — A tooling cache in front of the probe, with in-flight dedup

**Type:** Feature

`host_tooling_cache` module: `ToolingCache`, `ToolingKey { daemon_instance_id, os_user, github_hostname }`, a `Clock` trait with `SystemClock`. Answers are kept for `HOST_TOOLING_CACHE_TTL` (30 s), concurrent callers for one key share one probe, a caller dropped mid-probe stores nothing, and expired unheld slots are evicted so caller-supplied hostnames cannot grow the map. `get_host_tooling` goes through a private `cached_tooling` method. Failed readings are cached with the rest of the answer; a probe that panicked is kept as its message for the same window. Closes the backlog entry *The tooling probe has no cache and no in-flight dedup* (2026-09-06).

`service.rs` grew from 936 to 961 production lines (the cache wiring); the split is deferred and recorded in `oversized-file-service.md`. `host_tooling.rs` is unchanged at 775. See [host tooling probe](../host-tooling-probe.md#the-tooling-cache).

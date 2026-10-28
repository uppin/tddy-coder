# 2026-10-28 — The split context fetch carries links without reading them

**Type:** Fix

`split_context_from_codebase_host` maps `symlink_target` and leaves link entries out of the batch
read and the size/hash checks. PR [#784](https://github.com/uppin/tddy-coder/pull/784).

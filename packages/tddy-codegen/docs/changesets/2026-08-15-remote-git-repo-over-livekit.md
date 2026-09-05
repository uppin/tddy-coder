# 2026-08-15 — remote-git-repo-over-livekit

**Type:** Fix

`import_once`: `prost_build` hands every service in a `.proto` the same output buffer, so a file declaring two services emitted `use async_trait::async_trait;` twice and would not compile. "One service per file" was an accident rather than a decision; `auth.proto` is the first file to declare two. (tddy-codegen)

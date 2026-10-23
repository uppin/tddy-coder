# 2026-10-23 — A harness profile's provider_account is optional
**Type:** Fix

`HarnessProfile.provider_account` is `#[serde(default)]`: a profile that names no account loaded as an error and was silently skipped. See [harness profiles](../../../../docs/ft/daemon/harness-profiles.md) (#699, `#project-home` 4/7).

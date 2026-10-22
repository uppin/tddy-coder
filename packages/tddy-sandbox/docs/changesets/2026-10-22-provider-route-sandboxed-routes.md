# 2026-10-22 — A literal secret never reaches a plan's `Debug`

**Type:** Fix

`SecretSource` has a hand-written `Debug` that prints `Value(<redacted>)`, so a plan holding a
provider route's key can be logged and compared in tests safely. PR
[#670](https://github.com/uppin/tddy-coder/pull/670) (`#provider-route` 9/9).

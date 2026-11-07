# 2026-11-07 — One PR check per web package

Each JS workspace package now has its own PR check, `web / <package> tests`, which runs that
package's build and tests only when a PR affects it (its files, a workspace package it depends on, or
a shared input such as `bun.lock`). Unaffected packages report a skipped, green check. Replaces the
single `Web tests` check. See [ci.md § Web package checks](../../../dev/guides/ci.md#web-package-checks).

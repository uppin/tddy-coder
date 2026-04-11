# Changesets Applied

Wrapped changeset history for tddy-codegen.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-04-10** [Feature] **Conditional imports for unary-only services** — `TddyServiceGenerator` emits `Stream` / `StreamExt` / `mpsc` imports and `_method` only when service has bidi methods; fixes unused-import warnings for unary-only services such as `codex_oauth.CodexOAuthService`. (tddy-codegen)
- **2026-03-13** [Architecture Change] Dual-Transport Service Codegen — Renamed from tddy-livekit-codegen. TddyServiceGenerator: generates transport-agnostic service traits, RpcService server structs (per-method handlers, service name validation), tonic adapters (feature-gated). (tddy-codegen)

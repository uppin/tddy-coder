# 2026-07-24 — Sessions drawer Active/Remaining separator

- The open sessions drawer now splits its list into an **Active** partition (sessions whose status dot is green or yellow) and a **Remaining** partition (grey/disconnected), with collapsible `Active (N)` / `Remaining (M)` headers between them — Active expanded, Remaining collapsed by default — so live and attention-needing sessions stay at the top and finished ones tuck away one click below. See [session-drawer.md](../session-drawer.md).
- The separators appear only when the list holds both kinds; an all-active or all-finished list still renders as one flat list. Existing PR-stack group nesting is preserved within each partition, and bulk-select mode temporarily expands both partitions so delete continues to span them.

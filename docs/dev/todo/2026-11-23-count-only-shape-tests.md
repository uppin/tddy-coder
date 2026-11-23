# 2026-11-23 — count-only shape tests

**Category:** Test debt — delete
**Source:** operator decision, `#workqueue` lineage-fixer pass (2026-11-23)

**Rule (operator):** a test whose only assertion is a field, variant, method, line or entry **count**
(a shape or size-budget counter, not behaviour) is deleted, never bumped. Every `#workqueue` node
that grew a counted thing paid for it with a number change and a justification comment; none of
those numbers pinned anything a reviewer or another test does not already see.

**Audit** (read-only grep over `packages/*/tests` on the `#workqueue` 10/14 branch; behaviour tests
that merely check a length — result caps, window sizes, deadlines, "at most N events" — are excluded):

| Test | Where | What it counts |
|---|---|---|
| `each_handler_holds_no_more_fields_than_its_budget` (+ its `fields_of_struct` helper) | `packages/tddy-daemon-rpc/tests/rpc_handlers_shape.rs:161` | fields of the four RPC handler structs against a per-struct budget. **Already deleted at `#workqueue` 11/14** (`WorkQueueRpcHandler` 11 > 9); present on 9–10 only |
| `the_presenter_holds_five_state_groups_and_nothing_loose` | `packages/tddy-core/tests/core_foundations_shape.rs:186` | fields of `Presenter`, `assert_eq!(fields, 7)` |
| the closing `assert_eq!(declared.len(), RESIDUAL_METHODS.len())` of `connection_service_ends_at_exactly_the_residual` | `packages/tddy-service/tests/unbundle_service_split.rs:684` (assertion at ~718) | RPC methods on the four residual services. **Partly behaviour**: its first assertion (no method outside `RESIDUAL_METHODS`) names the strays and is kept; only the count assertion goes |

Not listed: the other eight tests in `rpc_handlers_shape.rs` (dependency edges, impl placement,
holder checks) and the `*_shape` suites' edge tests — they assert structure, not size.

**Recommendation:** remove these in one sweep (one commit, `test: drop count-only shape tests`),
by deleting the test and any helper only it used — do not adjust the numbers. Re-run the grep above
first, since a later node may have added another.

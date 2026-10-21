# 2026-10-21 — `tddy-accounts` `service.rs` is past the 500-line budget

**Category:** Deferred from `#provider-route` 1/9 (#662)
**Source:** `/pr-wrap` step 3.5 file-length gate on `feature/provider-route/accounts`

`packages/tddy-accounts/src/service.rs` went from **480 → 647 production lines** (counted to the
first `#[cfg(test)]`) in #662, which added `ConnectProviderAccount`, `ListProviderAccountModels` and
their helpers (`requested_account`, `non_blank`, `refusal_of_provider_account`).

**Why deferred:** the file is modified by later `#provider-route` nodes (#663–#670) as well. Splitting
it inside the stack would cascade the move through every dependent branch's diff as a conflict, which
the stack rule forbids (`/pr-wrap` step 4). Decompose it on a follow-up branch once the stack lands.

**What would close it:** move the provider-account half (the two RPC bodies' helpers and the
`ProviderAccountError` → `Status` mapping) into its own module beside `provider_account.rs`, and the
link half (`store_link`, `state_only`, `pending`, `link_status_for`) into one beside `linking.rs`,
via the `code-restructuring` skill, until `service.rs` is under 500 production lines.

**#663 (`#provider-route` 2/9):** grew `service.rs` **647 → 714 production lines**
(`list_provider_routes`, `with_route_support`). Still deferred for the same stack-shared reason:
later nodes (#666+) modify the file too.

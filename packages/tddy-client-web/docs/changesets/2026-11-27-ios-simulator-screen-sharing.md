# 2026-11-27 — The registry lends media; received-video stats on the media port (PR #1060)

**Type:** Feature

`ConnectionProviderRegistry.connectHost` returns the winning connection borrowing `media` from a
later connection that reaches the same host (`BorrowingMediaConnection`, `LENDABLE_CAPABILITIES`),
live on the lender's status and memoised per winner and lenders. `MediaPort` gains
`useReceivedVideoStats` and `ReceivedVideoStats`. Regenerated `host_pb`, `screen_sharing_pb`,
`screen_sharing_input_pb`. See [README.md § Media is lent](../README.md#media-is-lent-calls-are-not).

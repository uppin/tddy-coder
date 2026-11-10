//! The `jsonl` format: one conversation's lines, filtered, deduplicated and aggregated.

// TODO(claude-transcript): implement — fold a conversation's files under a `RecordSpec`: drop
// lines that are not JSON or fail `where`, keep the last line per `dedupe_by` key, sum each
// counter, take the last `model`, count the kept records as turns.

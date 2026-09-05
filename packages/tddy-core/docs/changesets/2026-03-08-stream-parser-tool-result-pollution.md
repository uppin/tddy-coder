# 2026-03-08 — Stream Parser Tool-Result Pollution

**Type:** Bug Fix

Fixed process_ndjson_stream to collect user tool_result content in a separate buffer, merging into result_text only when primary sources lack a structured-response block. Prevents file-read content (containing structured-response examples) from polluting the parse buffer. (tddy-core)

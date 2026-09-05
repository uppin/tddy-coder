# 2026-03-08 — Structured Response rfind

**Type:** Bug Fix

Changed parse_acceptance_tests_response and parse_green_response from find to rfind for locating structured-response blocks, preventing earlier blocks (e.g. system prompt examples in file reads) from being selected. (tddy-core)

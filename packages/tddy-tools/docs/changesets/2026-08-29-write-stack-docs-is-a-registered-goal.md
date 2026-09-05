# 2026-08-29 — `write-stack-docs` is a registered goal

**Type:** Bug Fix

it had no schema, so a wrong-shaped payload failed at a parse error inside the hook instead of naming the offending paths, and its prompt told the agent to submit YAML while `submit` parses JSON, refusing an agent that followed it; every test drove the hook directly and never the CLI, so the suite was green over a flow that could not work. `get-schema write-stack-docs` now answers, and a contract test pushes the prompt's own fenced example through the real `submit` binary so the two cannot drift apart.

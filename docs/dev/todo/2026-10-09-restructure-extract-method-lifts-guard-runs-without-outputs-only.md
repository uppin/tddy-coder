# 2026-10-09 — `extract_method` lifts a run of error guards only when the run binds nothing read after it

**Category:** Future enhancement
**Source:** #reshape 4/19 (`extract-method-clean`), decision F10 (guard lift)

#reshape 4 lifts a mid-function run of `return Err(..)` (or `return None`) guards into `fn name(..) -> Result<()>` called
as `name(..)?;`. A run that also declares a binding the code after it reads is **refused**, naming the binding:

```rust
    let raw: Value = serde_json::from_str(line).map_err(|e| malformed(e.to_string()))?;
    if !raw.is_object() {
        return Err(malformed("not an object"));
    }
    // … `raw` is read below: the run [let raw … }] has an output
```

The natural lift is `fn name(..) -> Result<Value>` with `let raw = name(..)?;`, and the same for several outputs (a
tuple, or the plan cuts smaller). A `let … else { return Err(..) };` guard that binds is the same case.

## Why deferred

rust-analyzer's output for a range with both outputs and early returns was not reproduced while planning #reshape 4: the
rewrite there is pinned to the one shape the server writes for an output-free run (`Option<Result<T, E>>`,
`if let Some(value) = … { return value; }`). Until it is measured, a plan cuts the run after the binding, which the refusal
says.

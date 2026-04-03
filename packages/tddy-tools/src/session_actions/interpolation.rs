//! Resolve argv template fragments from validated input (literals, `from` pointers, array rules).

use anyhow::{bail, Context, Result};
use log::{debug, info};
use serde_json::Value;

fn json_value_to_argv_token(v: &Value) -> Result<String> {
    match v {
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(if *b { "true" } else { "false" }.to_string()),
        Value::Null => bail!("interpolated value is null; omit fragment or use a literal"),
        Value::Array(_) | Value::Object(_) => {
            bail!(
                "interpolated value must be scalar for argv fragment, got {}",
                v
            )
        }
    }
}

/// Materialize argv strings from YAML `executor.argv` fragments and validated input.
pub fn materialize_argv_from_templates(
    template_fragments: &[Value],
    input: &Value,
) -> Result<Vec<String>> {
    info!(
        target: "tddy_tools::session_actions::interpolation",
        "materialize_argv_from_templates fragments={}",
        template_fragments.len()
    );
    let mut argv = Vec::with_capacity(template_fragments.len());
    for (i, frag) in template_fragments.iter().enumerate() {
        let obj = frag
            .as_object()
            .with_context(|| format!("argv[{i}] must be a JSON object"))?;
        let literal = obj.get("literal");
        let from = obj.get("from").and_then(|v| v.as_str());
        match (literal, from) {
            (Some(lit), None) => {
                let s =
                    json_value_to_argv_token(lit).with_context(|| format!("argv[{i}] literal"))?;
                argv.push(s);
            }
            (None, Some(ptr)) => {
                let p = if ptr.is_empty() {
                    ""
                } else if ptr.starts_with('/') {
                    ptr
                } else {
                    bail!("argv[{i}] JSON Pointer must start with '/', got {ptr:?}");
                };
                let resolved = input.pointer(p).with_context(|| {
                    format!("argv[{i}] pointer {p:?} not found in validated input")
                })?;
                argv.push(
                    json_value_to_argv_token(resolved)
                        .with_context(|| format!("argv[{i}] value at pointer {p}"))?,
                );
            }
            (Some(_), Some(_)) => bail!("argv[{i}] cannot have both literal and from"),
            (None, None) => bail!("argv[{i}] needs literal or from"),
        }
    }
    debug!(
        target: "tddy_tools::session_actions::interpolation",
        "materialized argv len={}",
        argv.len()
    );
    Ok(argv)
}

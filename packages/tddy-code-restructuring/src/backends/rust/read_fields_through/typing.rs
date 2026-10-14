//! Field mode's typing: the type of a field as the server's hover states it, and whether the state
//! value's field agrees with the host's (RS7, and the `&` rule).

use serde_json::Value;

use super::super::server_defect;

/// How a state field's type relates to the host field's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Agreement {
    /// The same type: a `&self.f` stays `&state.f`.
    Same,
    /// The state holds a reference to the host's type: `&self.f` becomes `state.f`.
    ByReference,
    /// Anything else: refused (RS7).
    Differs,
}

/// The type a hover over a field access states, from the `name: Type` line of its code block;
/// a hover without one is the server's answer being unusable.
pub(super) fn field_type(hover: &Value) -> crate::Result<String> {
    let mut texts = Vec::new();
    collect_texts(hover.get("contents").unwrap_or(&Value::Null), &mut texts);
    let found = texts
        .iter()
        .flat_map(|text| candidate_lines(text))
        .find_map(field_line_type);
    found.ok_or_else(|| {
        server_defect(format!(
            "its hover over a field has no `name: Type` line in a code block: {hover}"
        ))
    })
}

/// Every string a hover's `contents` holds: markup, a marked string or a list of either.
fn collect_texts(contents: &Value, texts: &mut Vec<String>) {
    match contents {
        Value::String(text) => texts.push(text.clone()),
        Value::Array(entries) => entries.iter().for_each(|entry| collect_texts(entry, texts)),
        Value::Object(fields) => {
            if let Some(Value::String(text)) = fields.get("value") {
                texts.push(text.clone());
            }
        }
        _ => {}
    }
}

/// The lines that may state the field: those inside the fenced code blocks of a markdown text, or
/// every line of a plain one (what a client that declares no markdown is answered with).
fn candidate_lines(text: &str) -> Vec<&str> {
    let fenced = text.contains("```");
    let mut inside = false;
    text.lines()
        .filter(|line| {
            if line.trim_start().starts_with("```") {
                inside = !inside;
                return false;
            }
            inside || !fenced
        })
        .collect()
}

/// The type of a `[pub] name: Type` line, a trailing `//` comment (the server's layout note) cut.
fn field_line_type(line: &str) -> Option<String> {
    let line = line.split("//").next().unwrap_or(line).trim();
    let line = match line.strip_prefix("pub") {
        Some(rest) if rest.starts_with('(') => rest.split_once(')')?.1.trim_start(),
        Some(rest) if rest.starts_with(char::is_whitespace) => rest.trim_start(),
        _ => line,
    };
    let (name, field_type) = line.split_once(':')?;
    let is_a_name = !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_');
    let field_type = field_type.trim().trim_end_matches(',').trim();
    (is_a_name && !field_type.is_empty() && !field_type.starts_with(':'))
        .then(|| field_type.to_string())
}

/// How `state` relates to `host`, lifetimes ignored.
pub(super) fn agreement(host: &str, state: &str) -> Agreement {
    let (host, state) = (without_lifetimes(host), without_lifetimes(state));
    if state == host {
        Agreement::Same
    } else if state == format!("&{host}") || state == format!("&mut {host}") {
        Agreement::ByReference
    } else {
        Agreement::Differs
    }
}

/// `written` with every lifetime removed and its spacing made one: `&'a mut Arc<Store>` and
/// `&mut Arc< Store >` both read `&mut Arc<Store>`.
pub(super) fn without_lifetimes(written: &str) -> String {
    let chars: Vec<char> = written.chars().collect();
    let mut kept = String::new();
    let mut at = 0;
    while at < chars.len() {
        let is_lifetime = chars[at] == '\''
            && chars
                .get(at + 1)
                .is_some_and(|next| next.is_alphabetic() || *next == '_');
        if !is_lifetime {
            kept.push(chars[at]);
            at += 1;
            continue;
        }
        at += 1;
        while chars
            .get(at)
            .is_some_and(|next| next.is_alphanumeric() || *next == '_')
        {
            at += 1;
        }
        // A lifetime argument takes its comma with it: `Foo<'a, T>` is `Foo<T>`.
        while chars.get(at).is_some_and(|next| next.is_whitespace()) {
            at += 1;
        }
        if chars.get(at) == Some(&',') {
            at += 1;
        }
    }
    let kept = kept.replace("<>", "").replace("< ", "<").replace(" >", ">");
    kept.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" ,", ",")
        .replace("& ", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_with_a_lifetime_to_the_hosts_type_agrees_by_reference() {
        let agreements = [
            agreement("Arc<Store>", "&'a Arc<Store>"),
            agreement("Arc<Store>", "&Arc<Store>"),
            agreement("Duration", "Duration"),
            agreement("Arc<Vec<u8>>", "Vec<u8>"),
        ];

        assert_eq!(
            agreements,
            [
                Agreement::ByReference,
                Agreement::ByReference,
                Agreement::Same,
                Agreement::Differs
            ]
        );
    }

    #[test]
    fn the_type_is_read_from_the_field_line_of_the_hover() {
        let hover = serde_json::json!({
            "contents": {
                "kind": "markdown",
                "value": "```rust\napp::host::HostState\n```\n\n```rust\npub config: &'a Config\n```"
            }
        });

        assert_eq!(field_type(&hover).expect("a field line"), "&'a Config");
    }

    #[test]
    fn a_hover_without_a_field_line_is_refused() {
        let hover = serde_json::json!({ "contents": { "kind": "markdown", "value": "no type" } });

        assert!(field_type(&hover).is_err());
    }
}

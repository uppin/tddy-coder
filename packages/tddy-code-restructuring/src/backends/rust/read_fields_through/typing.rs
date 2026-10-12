//! Field mode's typing: the type of a field as the server's hover states it, and whether the state
//! value's field agrees with the host's (RS7, and the `&` rule).

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
pub(super) fn field_type(hover: &serde_json::Value) -> crate::Result<String> {
    // TODO(reshape-methods-leave-type): implement
    let _ = hover;
    todo!("TODO(reshape-methods-leave-type): implement field_type")
}

/// How `state` relates to `host`, lifetimes ignored.
pub(super) fn agreement(host: &str, state: &str) -> Agreement {
    // TODO(reshape-methods-leave-type): implement
    let _ = (host, state);
    todo!("TODO(reshape-methods-leave-type): implement agreement")
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

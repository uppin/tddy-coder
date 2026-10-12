//! What a caller's receiver is, from the type the server's hover states for it (DS8).

/// How a receiver relates to the block's self type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ReceiverType {
    /// `&T` or `&mut T`: passed as it is.
    Reference,
    /// `T` itself, a place: borrowed at the call by the member's receiver kind.
    Place,
    /// Anything else (`Box<T>`, `Rc<T>`, …), named in the refusal.
    Other(String),
}

/// How the receiver whose hover type is `hover_type` relates to `self_type`, lifetimes ignored.
pub(super) fn receiver_type(hover_type: &str, self_type: &str) -> ReceiverType {
    // TODO(reshape-backend-session): implement
    let _ = self_type;
    ReceiverType::Other(hover_type.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_is_passed_a_place_is_borrowed_and_a_pointer_is_named() {
        // Given the hover types of six receivers of `Host` methods
        let types = [
            "&mut Host",
            "&'a Host",
            "&Host",
            "Host",
            "Box<Host>",
            "Rc<Host>",
        ];

        // When each is read against `Host`
        let read: Vec<ReceiverType> = types.iter().map(|t| receiver_type(t, "Host")).collect();

        // Then references pass, the place is borrowed, and the pointers are named
        assert_eq!(
            read,
            [
                ReceiverType::Reference,
                ReceiverType::Reference,
                ReceiverType::Reference,
                ReceiverType::Place,
                ReceiverType::Other("Box<Host>".to_string()),
                ReceiverType::Other("Rc<Host>".to_string()),
            ]
        );
    }
}

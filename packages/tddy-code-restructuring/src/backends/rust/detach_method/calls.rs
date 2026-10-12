//! The callers: what shape each reference the server reports has, and the call it becomes.
//!
//! A method call `R.m(args)` becomes `<callee>(R', args)`, the arguments byte for byte; a path call
//! `Self::m(args)` / `T::m(args)` becomes `<callee>(args)`; a path used as a value becomes
//! `<callee>`. `<callee>` is `m` in the function's own module and `M::m` elsewhere, with the `use`
//! of `M` the caller's file needs.

use std::ops::Range;

use crate::Result;

/// The shape of one reference to a detached member, in byte offsets of the caller's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Reached {
    /// `R.m(args)`: `receiver` spans `R`, `open` is the offset of the `(`.
    MethodCall { receiver: Range<usize>, open: usize },
    /// `Self::m(args)` / `T::m(args)`: `path` spans everything before the `(`.
    PathCall { path: Range<usize> },
    /// `T::m` named but not called: `path` spans it.
    PathValue { path: Range<usize> },
}

/// The shape of the reference to `name` at `offset` of `masked` (comments and strings masked); a
/// reference inside a macro invocation is refused (DS9).
pub(super) fn reached_at(masked: &str, offset: usize, name: &str) -> Result<Reached> {
    // TODO(reshape-backend-session): implement
    let _ = (masked, offset, name);
    Err(crate::RestructureError::UnsupportedOp {
        backend: "`detach_method` is not implemented yet (TODO(reshape-backend-session))"
            .to_string(),
        op: "DetachMethod".to_string(),
    })
}

/// The text the reference `reached` in `text` becomes, calling `callee`; `borrow` (`&mut ` or `&`)
/// prefixes a receiver that is a place rather than a reference.
pub(super) fn call_text(
    text: &str,
    reached: &Reached,
    callee: &str,
    borrow: Option<&str>,
) -> String {
    // TODO(reshape-backend-session): implement
    let _ = (text, reached, callee, borrow);
    String::new()
}

/// How a caller in `caller_module` names the function `name` of `function_module` (module paths from
/// the crate root), and the `use` line it needs when it is not the function's own module.
pub(super) fn callee_from(
    caller_module: &[String],
    function_module: &[String],
    name: &str,
) -> (String, Option<String>) {
    // TODO(reshape-backend-session): implement
    let _ = (caller_module, function_module, name);
    (String::new(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn the_shape_at(text: &str, needle: &str, name: &str) -> Reached {
        let offset =
            text.find(needle).expect("the needle is in the text") + needle.len() - name.len();
        reached_at(text, offset, name).unwrap_or_else(|refusal| panic!("refused: {refusal}"))
    }

    #[test]
    fn reads_a_method_call_a_path_call_and_a_path_value() {
        // Given a method call, a path call and a path used as a value
        let text = "fn f(h: &mut Host) {\n    h.bump(1);\n    Self::pure(2);\n    let g = Host::pure;\n}\n";

        // When the shape of each reference is read
        let shapes = [
            the_shape_at(text, "h.bump", "bump"),
            the_shape_at(text, "Self::pure", "pure"),
            the_shape_at(text, "Host::pure", "pure"),
        ];

        // Then each is read with the spans its rewrite needs
        let receiver = text.find("h.bump").expect("receiver");
        let open = text.find("bump(").expect("call") + "bump".len();
        let path_call = text.find("Self::pure").expect("path call");
        let path_value = text.find("Host::pure").expect("path value");
        assert_eq!(
            shapes,
            [
                Reached::MethodCall {
                    receiver: receiver..receiver + 1,
                    open
                },
                Reached::PathCall {
                    path: path_call..path_call + "Self::pure".len()
                },
                Reached::PathValue {
                    path: path_value..path_value + "Host::pure".len()
                },
            ]
        );
    }

    #[test]
    fn refuses_a_reference_inside_a_macro_invocation() {
        // Given a call written inside `format!`
        let text = "fn f(h: &Host) -> String {\n    format!(\"{}\", h.total())\n}\n";
        let offset = text.find("total").expect("the reference");

        // When its shape is read
        let refused = reached_at(text, offset, "total");

        // Then it is refused as inside a macro invocation
        assert!(
            refused
                .as_ref()
                .is_err_and(|refusal| refusal.to_string().contains("inside a macro invocation")),
            "{refused:?}"
        );
    }

    #[test]
    fn writes_the_receiver_first_and_keeps_the_arguments_byte_for_byte() {
        // Given a method call on a reference and one on an owned place
        let text = "    h.bump(1,  /* twice */ 2);\n";
        let reached = Reached::MethodCall {
            receiver: 4..5,
            open: 10,
        };

        // When each is rewritten to call `host::bump`
        let on_a_reference = call_text(text, &reached, "host::bump", None);
        let on_a_place = call_text(text, &reached, "host::bump", Some("&mut "));

        // Then the receiver is the first argument, borrowed when it is a place
        assert_eq!(
            (on_a_reference.as_str(), on_a_place.as_str()),
            (
                "host::bump(h, 1,  /* twice */ 2)",
                "host::bump(&mut h, 1,  /* twice */ 2)"
            )
        );
    }

    #[test]
    fn writes_a_call_with_no_arguments_as_the_receiver_alone() {
        // Given a method call with no arguments
        let text = "    h.total()\n";
        let reached = Reached::MethodCall {
            receiver: 4..5,
            open: 11,
        };

        // When it is rewritten
        // Then the receiver is the only argument
        assert_eq!(call_text(text, &reached, "total", None), "total(h)");
    }

    #[test]
    fn names_the_function_bare_in_its_module_and_through_its_module_elsewhere() {
        // Given the function's module and a caller in it, in its parent and in a sibling
        let module = |path: &[&str]| path.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        let host = module(&["app", "host"]);

        // When each caller names `bump`
        let callees = [
            callee_from(&host, &host, "bump"),
            callee_from(&module(&["app"]), &host, "bump"),
            callee_from(&module(&["app", "tick"]), &host, "bump"),
        ];

        // Then the bare name serves at home, and `host::bump` with its `use` serves elsewhere
        assert_eq!(
            callees,
            [
                ("bump".to_string(), None),
                (
                    "host::bump".to_string(),
                    Some("use crate::host;".to_string())
                ),
                (
                    "host::bump".to_string(),
                    Some("use crate::host;".to_string())
                ),
            ]
        );
    }
}

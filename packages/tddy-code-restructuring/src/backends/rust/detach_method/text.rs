//! The function a member becomes, written from the member's own text: the receiver read and
//! rewritten, `self` and `Self` rewritten, the text re-indented — and the members that cannot be
//! detached at all (DS1-DS5), refused from the block header and the members' text.

use crate::Result;

/// How a member takes its receiver, which decides the parameter it gets and how an owned receiver
/// is borrowed at a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Receiver {
    /// `&mut self` (or `&'a mut self`, `self: &mut Self`): the parameter is `name: &mut T`.
    Unique,
    /// `&self`: the parameter is `name: &T`.
    Shared,
    /// No receiver: an associated function keeps its parameters.
    Absent,
}

/// How the member whose signature (visibility to the body's `{`) is `signature` takes its receiver;
/// a receiver taken by value is refused (DS1).
pub(super) fn receiver_of(signature: &str) -> Result<Receiver> {
    // TODO(reshape-backend-session): implement
    let _ = signature;
    Err(not_implemented())
}

/// Refuse every member of the block headed `header` that cannot be detached, all at once: a
/// by-value receiver (DS1), a generic header or `where` clause (DS2), a trait impl (DS3), a `pub`
/// member (DS4), an associated const or type (DS5). `members` are `(name, text)` pairs.
pub(super) fn refuse_undetachable(header: &str, members: &[(&str, &str)]) -> Result<()> {
    // TODO(reshape-backend-session): implement
    let _ = (header, members);
    Err(not_implemented())
}

/// The free function `member_text` (attached trivia included, indented by `indent` columns) becomes
/// with its receiver named `name` and its self type `self_type`.
pub(super) fn function_text(
    member_text: &str,
    name: &str,
    self_type: &str,
    indent: usize,
) -> Result<String> {
    // TODO(reshape-backend-session): implement — the receiver, `read_fields_through::edits::rebound`
    // for `self` and `Self`, the re-indent past string-literal lines.
    let _ = (member_text, name, self_type, indent);
    Err(not_implemented())
}

fn not_implemented() -> crate::RestructureError {
    crate::RestructureError::UnsupportedOp {
        backend: "`detach_method` is not implemented yet (TODO(reshape-backend-session))"
            .to_string(),
        op: "DetachMethod".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn the_receiver_of(signature: &str) -> Receiver {
        receiver_of(signature)
            .unwrap_or_else(|refusal| panic!("`{signature}` was refused: {refusal}"))
    }

    #[test]
    fn reads_a_unique_a_shared_and_an_absent_receiver() {
        // Given a signature for each kind of receiver a member can be detached with
        let signatures = [
            "pub(super) fn bump(&mut self, by: u32) -> u32 {",
            "fn bump<'a>(&'a mut self) {",
            "fn bump(self: &mut Self) {",
            "fn total(&self) -> u32 {",
            "fn pure(x: u32) -> u32 {",
        ];

        // When each receiver is read
        let read: Vec<Receiver> = signatures.iter().map(|s| the_receiver_of(s)).collect();

        // Then each is read as the parameter it becomes
        assert_eq!(
            read,
            [
                Receiver::Unique,
                Receiver::Unique,
                Receiver::Unique,
                Receiver::Shared,
                Receiver::Absent
            ]
        );
    }

    #[test]
    fn refuses_a_receiver_taken_by_value() {
        // Given two builders that consume their receiver
        let builders = [
            "pub fn with_trace(mut self, sink: fn(&str)) -> Self {",
            "fn into_parts(self) -> (u32, u32) {",
        ];

        // When each receiver is read
        let refusals: Vec<String> = builders
            .iter()
            .map(|signature| match receiver_of(signature) {
                Err(refusal) => refusal.to_string(),
                Ok(read) => panic!("`{signature}` was read as {read:?}"),
            })
            .collect();

        // Then each says a function over a reference cannot take it
        assert!(
            refusals
                .iter()
                .all(|refusal| refusal.contains("takes `self` by value")),
            "{refusals:?}"
        );
    }

    const A_MEMBER: &str = concat!(
        "    /// Bump the count, up to the limit.\n",
        "    #[inline]\n",
        "    pub(super) fn bump(&mut self, by: u32) -> u32 {\n",
        "        // self.n is the count\n",
        "        self.n += by;\n",
        "        Self::LIMIT.min(self.n)\n",
        "    }\n",
    );

    #[test]
    fn writes_the_member_as_a_function_over_the_named_parameter() {
        // Given a documented, attributed member reading `self` and `Self`
        // When it is written as a function over `session`
        let written = function_text(A_MEMBER, "session", "Host", 4).expect("the member detaches");

        // Then the trivia and visibility stay, the receiver is the parameter, the comment is
        // untouched, and the text sits at the module's indentation
        assert_eq!(
            written,
            concat!(
                "/// Bump the count, up to the limit.\n",
                "#[inline]\n",
                "pub(super) fn bump(session: &mut Host, by: u32) -> u32 {\n",
                "    // self.n is the count\n",
                "    session.n += by;\n",
                "    Host::LIMIT.min(session.n)\n",
                "}\n",
            )
        );
    }

    #[test]
    fn leaves_the_lines_of_a_string_literal_where_they_were() {
        // Given a member whose string literal runs over two lines
        let member = concat!(
            "    fn banner(&self) -> String {\n",
            "        format!(\"{}:\n",
            "    continued\", self.name)\n",
            "    }\n",
        );

        // When it is written as a function over `session`
        let written = function_text(member, "session", "Host", 4).expect("the member detaches");

        // Then the line that starts inside the literal keeps its bytes
        assert_eq!(
            written,
            concat!(
                "fn banner(session: &Host) -> String {\n",
                "    format!(\"{}:\n",
                "    continued\", session.name)\n",
                "}\n",
            )
        );
    }

    #[test]
    fn refuses_a_generic_header_a_trait_impl_a_pub_member_and_an_associated_const_naming_each() {
        // Given four blocks, each with one member that cannot be detached
        let refusals: Vec<String> = [
            (
                "impl<T> Host<T> {",
                ("get", "    fn get(&self) -> u32 {\n        1\n    }\n"),
            ),
            (
                "impl Display for Host {",
                ("fmt", "    fn fmt(&self) -> u32 {\n        1\n    }\n"),
            ),
            (
                "impl Host {",
                ("get", "    pub fn get(&self) -> u32 {\n        1\n    }\n"),
            ),
            ("impl Host {", ("LIMIT", "    const LIMIT: u32 = 3;\n")),
        ]
        .iter()
        .map(
            |(header, member)| match refuse_undetachable(header, &[*member]) {
                Err(refusal) => refusal.to_string(),
                Ok(()) => panic!("`{header}` with `{}` was accepted", member.0),
            },
        )
        .collect();

        // Then each refusal says why
        let reasons = [
            "carries generic parameters or a `where` clause",
            "is a member of `impl Display for Host`: trait members stay methods",
            "`Host::get` is `pub` and may have callers outside the workspace",
            "`Host::LIMIT` is an associated const or type: there is nothing to detach",
        ];
        let unexplained: Vec<(&String, &&str)> = refusals
            .iter()
            .zip(&reasons)
            .filter(|(refusal, reason)| !refusal.contains(**reason))
            .collect();
        assert!(unexplained.is_empty(), "{unexplained:?}");
    }

    #[test]
    fn accepts_a_private_and_a_restricted_member_of_a_plain_block() {
        // Given a plain block's private and `pub(super)` members
        let members = [
            ("get", "    fn get(&self) -> u32 {\n        1\n    }\n"),
            (
                "put",
                "    pub(super) fn put(&mut self, n: u32) {\n        let _ = n;\n    }\n",
            ),
        ];

        // When they are checked
        // Then neither is refused
        assert!(refuse_undetachable("impl Host {", &members).is_ok());
    }
}

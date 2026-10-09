//! `retarget_impl`'s forwarding delegator (`variant: "leave_delegator"` with `expr`): the method
//! left in each moved method's slot of the old block so its callers keep compiling, the members
//! that cannot be forwarded (S7), and the delegators nothing calls.
//!
//! The delegator's signature and outer attributes are byte ranges of the moved member; the only
//! text authored is the forwarding body.

use std::ops::Range;

use super::super::item_move::sites::Site;
use crate::Result;

/// Refuse, before any edit, every moved member a forwarding method cannot be written for (S7): a
/// parameter written as a pattern, an associated const or type, a `const fn`. `members` are
/// `(name, text)` of each moved member.
pub(super) fn refuse_unforwardable(members: &[(&str, &str)]) -> Result<()> {
    // TODO(reshape-methods-leave-type): implement S7
    let _ = members;
    todo!("TODO(reshape-methods-leave-type): implement refuse_unforwardable")
}

/// The forwarding method for one moved member: its outer attributes but no doc comment, its
/// signature byte for byte, and the body `<expr>.<name>(<argument names>)` (`.await`ed for an
/// `async fn`), or `<new_type>::<name>(…)` for a member with no receiver.
pub(super) fn forwarding_method(member_text: &str, expr: &str, new_type: &str) -> Result<String> {
    // TODO(reshape-methods-leave-type): implement
    let _ = (member_text, expr, new_type);
    todo!("TODO(reshape-methods-leave-type): implement forwarding_method")
}

/// One note per moved member whose every reference lies inside the moved range of `file`: its
/// delegator has no caller in the workspace.
pub(super) fn dead_delegators(
    sites: &[Site],
    file: &str,
    moved: Range<usize>,
    members: &[&str],
    old_type: &str,
) -> Vec<String> {
    // TODO(reshape-methods-leave-type): implement
    let _ = (sites, file, moved, members, old_type);
    todo!("TODO(reshape-methods-leave-type): implement dead_delegators")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_method_forwards_through_the_expression_keeping_its_signature_and_attributes_but_not_its_docs(
    ) {
        let member = "    /// Replace the count.\n    #[inline]\n    pub fn put(&mut self, v: u32) {\n        self.n = v;\n    }\n";

        let delegator = forwarding_method(member, "self.roster", "Roster").expect("it forwards");

        assert_eq!(
            delegator,
            "    #[inline]\n    pub fn put(&mut self, v: u32) {\n        self.roster.put(v)\n    }\n"
        );
    }

    #[test]
    fn an_async_method_is_awaited_and_an_associated_function_forwards_through_the_new_type() {
        let async_member = "    pub async fn load(&self, id: u32) -> u32 {\n        id\n    }\n";
        let associated = "    pub fn build(n: u32) -> Self {\n        Self { n }\n    }\n";

        let forwarded = [
            forwarding_method(async_member, "self.roster()", "Roster").expect("it forwards"),
            forwarding_method(associated, "self.roster()", "Roster").expect("it forwards"),
        ];

        assert_eq!(
            forwarded,
            [
                "    pub async fn load(&self, id: u32) -> u32 {\n        self.roster().load(id).await\n    }\n",
                "    pub fn build(n: u32) -> Self {\n        Roster::build(n)\n    }\n",
            ]
        );
    }

    #[test]
    fn a_pattern_parameter_an_associated_const_and_a_const_fn_are_refused_together() {
        let members = [
            (
                "pair",
                "    fn pair(&self, (a, b): (u32, u32)) -> u32 {\n        a + b\n    }\n",
            ),
            ("LIMIT", "    const LIMIT: u32 = 3;\n"),
            (
                "zero",
                "    const fn zero(&self) -> u32 {\n        0\n    }\n",
            ),
        ];

        let refusal = refuse_unforwardable(&members)
            .map_err(|error| error.to_string())
            .expect_err("none of them can forward");

        assert!(
            refusal.contains("`pair` takes `(a, b)` as a pattern")
                && refusal.contains("`LIMIT` is an associated const or type")
                && refusal.contains("`zero` is a `const fn`"),
            "{refusal}"
        );
    }

    #[test]
    fn a_member_referenced_only_inside_the_moved_range_has_a_dead_delegator() {
        let sites = [
            Site {
                path: "src/host.rs".to_string(),
                offset: 40,
                name: "get".to_string(),
            },
            Site {
                path: "src/caller.rs".to_string(),
                offset: 10,
                name: "put".to_string(),
            },
        ];

        let notes = dead_delegators(&sites, "src/host.rs", 20..80, &["get", "put"], "Host");

        assert_eq!(
            notes,
            ["retarget_impl: the delegator `Host::get` has no caller in the workspace: remove it, or retarget without leave_delegator"]
        );
    }
}

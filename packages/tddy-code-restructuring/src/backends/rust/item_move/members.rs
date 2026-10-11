//! The fields and `impl` members a move splits from the code that uses them.
//!
//! A move widens the module-level items it connects; privacy that belongs to a member does not
//! follow. A private field of a moved struct read by an `impl` that stayed, or a private method that
//! stayed and is called by the moved code, stops compiling (`E0616`, `E0624`, `E0451`) unless it is
//! widened too. The outline already lists the members under each struct and `impl`, so this reads
//! them there ([`members_of`]), and widens each by as little as its users need ([`widen_members`]):
//! the same [`Scope`](super::scope::Scope) reading the items get.
//!
//! The widening is a function of the text and of who uses each member, so it is shared: an
//! operation that moves members between `impl` blocks widens them the same way.

// TODO(reshape-widen-same-crate): remove once `move_item` surveys members (milestone M4).
#![allow(dead_code)]

use std::ops::RangeInclusive;

use serde_json::Value;

use super::text::Edit;
use crate::edit::VisibilityChange;
use crate::Result;

/// A named member of a struct or an inherent `impl`, as the outline reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Member {
    /// The type the member belongs to: the struct, or the self type of the `impl`.
    pub(crate) owner: String,
    pub(crate) name: String,
    /// Where the server reports the member's name, which is where a reference query asks about it.
    pub(crate) position: Value,
    /// The visibility as written: empty for private.
    pub(crate) visibility: String,
}

/// A member, where it is written and where it ends up, and the modules that use it from the other
/// side of the move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReachedMember {
    pub(crate) member: Member,
    /// The module the member is written in before the move, below the crate root.
    pub(crate) written_in: Vec<String>,
    /// The module the member is written in after the move: the destination for a member that
    /// moves, the source for one that stays.
    pub(crate) lands_in: Vec<String>,
    /// The modules that use the member from the other side of the move.
    pub(crate) users: Vec<Vec<String>>,
}

/// The edits that widen the members, and the report of every widening.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MemberWidening {
    pub(crate) edits: Vec<Edit>,
    pub(crate) report: Vec<VisibilityChange>,
}

/// The private members of the structs and inherent `impl` blocks whose root symbol starts inside
/// `starting_in` (`inside`), or outside it (`!inside`), in file order.
///
/// Left out: the members of a trait `impl` and of a trait, which have no visibility of their own;
/// enum variants; and every member written `pub`, `pub(crate)` or with a relative visibility, which
/// the move's rebase keeps meaning what it meant.
pub(crate) fn members_of(
    symbols: &Value,
    text: &str,
    starting_in: RangeInclusive<u32>,
    inside: bool,
) -> Vec<Member> {
    let _ = (symbols, text, starting_in, inside);
    // TODO(reshape-widen-same-crate): implement
    todo!("read the members of the structs and inherent impls from the outline")
}

/// The edits to `text` that let every user of each member reach it, and the report of each one.
///
/// A member that is private where it is written and moves reads as private where it lands; its
/// scope is then widened until it covers every module in `users`, and spelled for `lands_in`. A
/// member whose scope does not change gets no edit and no report line. The report names the member
/// by its type: `Counter::count`.
pub(crate) fn widen_members(text: &str, reached: &[ReachedMember]) -> Result<MemberWidening> {
    let _ = (text, reached);
    // TODO(reshape-widen-same-crate): implement
    todo!("widen each member to the scope its users need")
}

/// The edit that gives the declaration of `member` the visibility `to`.
///
/// The keyword goes directly before the member's name, after any attribute written on the same
/// line: a field has nothing between its visibility and its name, which is what an item's
/// visibility edit refuses.
pub(crate) fn member_visibility_edit(text: &str, member: &Member, to: &str) -> Result<Edit> {
    let _ = (text, member, to);
    // TODO(reshape-widen-same-crate): implement
    todo!("write the visibility before the member's name")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::text::applied;
    use super::*;

    /// A module path below the crate root, written `a::b`; the crate root is the empty string.
    fn module(path: &str) -> Vec<String> {
        path.split("::")
            .filter(|segment| !segment.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// The member `owner::name` whose name the server reports at the 0-based `line` and `character`.
    fn a_member(owner: &str, name: &str, line: u32, character: u32) -> Member {
        Member {
            owner: owner.to_string(),
            name: name.to_string(),
            position: json!({ "line": line, "character": character }),
            visibility: String::new(),
        }
    }

    /// An outline symbol of `kind` named `name`, spanning the 0-based lines `first..=last`, its name
    /// at `name_at` on the first line, with `children`.
    fn a_symbol(
        name: &str,
        kind: u64,
        (first, last): (u32, u32),
        name_at: u32,
        children: Vec<Value>,
    ) -> Value {
        json!({
            "name": name,
            "kind": kind,
            "range": { "start": { "line": first, "character": 0 }, "end": { "line": last, "character": 1 } },
            "selectionRange": {
                "start": { "line": first, "character": name_at },
                "end": { "line": first, "character": name_at + name.len() as u32 },
            },
            "children": children,
        })
    }

    const STRUCT: u64 = 23;
    const FIELD: u64 = 8;
    const IMPL: u64 = 19;
    const METHOD: u64 = 6;
    const ENUM: u64 = 10;
    const VARIANT: u64 = 22;

    fn the_names_of(members: &[Member]) -> Vec<String> {
        members
            .iter()
            .map(|member| format!("{}::{}", member.owner, member.name))
            .collect()
    }

    #[test]
    fn a_moved_private_field_is_widened_where_it_lands_to_cover_the_module_left_behind() {
        // Given a private field of a struct moving from `pairing` to `answers`, read by `pairing`
        let text = "pub struct Counter {\n    count: u32,\n}\n";
        let reached = ReachedMember {
            member: a_member("Counter", "count", 1, 4),
            written_in: module("pairing"),
            lands_in: module("answers"),
            users: vec![module("pairing")],
        };

        // When the members are widened
        let widened = widen_members(text, &[reached]).expect("the field is widened");

        // Then it is visible to the crate, no wider, and the run says so
        assert_eq!(
            applied(text, &widened.edits).expect("the edits apply"),
            "pub struct Counter {\n    pub(crate) count: u32,\n}\n"
        );
        assert_eq!(
            widened.report,
            [VisibilityChange {
                item: "Counter::count".to_string(),
                from: "private".to_string(),
                to: "pub(crate)".to_string(),
            }]
        );
    }

    #[test]
    fn a_kept_private_method_is_widened_where_it_stays_to_cover_the_destination() {
        // Given a private method that stays in `pairing`, called from code moving to `answers`
        let text = "impl Counter {\n    fn peek(&self) -> u32 {\n        self.count\n    }\n}\n";
        let reached = ReachedMember {
            member: a_member("Counter", "peek", 1, 7),
            written_in: module("pairing"),
            lands_in: module("pairing"),
            users: vec![module("answers")],
        };

        // When the members are widened
        let widened = widen_members(text, &[reached]).expect("the method is widened");

        // Then the method is visible to the crate where it stays
        assert_eq!(
            applied(text, &widened.edits).expect("the edits apply"),
            "impl Counter {\n    pub(crate) fn peek(&self) -> u32 {\n        self.count\n    }\n}\n"
        );
        assert_eq!(
            widened.report,
            [VisibilityChange {
                item: "Counter::peek".to_string(),
                from: "private".to_string(),
                to: "pub(crate)".to_string(),
            }]
        );
    }

    #[test]
    fn a_member_whose_users_all_sit_under_its_new_module_keeps_its_visibility() {
        // Given a private field that stays in `pairing`, read by code moving into its child `inner`
        let text = "pub struct Counter {\n    count: u32,\n}\n";
        let reached = ReachedMember {
            member: a_member("Counter", "count", 1, 4),
            written_in: module("pairing"),
            lands_in: module("pairing"),
            users: vec![module("pairing::inner")],
        };

        // When the members are widened
        let widened = widen_members(text, &[reached]).expect("nothing needs widening");

        // Then a child sees its parent's private field, so nothing changes
        assert_eq!(widened, MemberWidening::default());
    }

    #[test]
    fn a_field_gets_its_keyword_written_before_its_name_after_any_same_line_attribute() {
        // Given a field with no keyword, one behind an attribute, and one with a relative keyword
        let cases = [
            ("    count: u32,\n", "    pub(crate) count: u32,\n"),
            (
                "    #[allow(dead_code)] count: u32,\n",
                "    #[allow(dead_code)] pub(crate) count: u32,\n",
            ),
            (
                "    pub(super) count: u32,\n",
                "    pub(crate) count: u32,\n",
            ),
        ];

        for (written, expected) in cases {
            let at = written.find("count").expect("the field is named") as u32;
            let member = a_member("Counter", "count", 0, at);

            // When the field is given `pub(crate)`
            let edit = member_visibility_edit(written, &member, "pub(crate)")
                .expect("a field's visibility is written in place");

            // Then the keyword stands directly before the name
            assert_eq!(
                applied(written, &[edit]).expect("the edit applies"),
                expected
            );
        }
    }

    #[test]
    fn reads_the_fields_of_a_struct_and_the_members_of_an_inherent_impl_but_nothing_of_a_trait_impl_or_an_enum(
    ) {
        // Given a struct, its inherent and trait `impl`s, and an enum, each with private and
        // public members
        let text = concat!(
            "pub struct Counter {\n",                                  // 0
            "    count: u32,\n",                                       // 1
            "    pub total: u32,\n",                                   // 2
            "}\n",                                                     // 3
            "impl Counter {\n",                                        // 4
            "    fn peek(&self) -> u32 { self.count }\n",              // 5
            "    pub(super) fn reset(&mut self) { self.count = 0 }\n", // 6
            "}\n",                                                     // 7
            "impl std::fmt::Display for Counter {\n",                  // 8
            "    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { Ok(()) }\n", // 9
            "}\n",                // 10
            "pub enum Shape {\n", // 11
            "    Round,\n",       // 12
            "}\n",                // 13
        );
        let outline = json!([
            a_symbol(
                "Counter",
                STRUCT,
                (0, 3),
                11,
                vec![
                    a_symbol("count", FIELD, (1, 1), 4, vec![]),
                    a_symbol("total", FIELD, (2, 2), 8, vec![]),
                ]
            ),
            a_symbol(
                "impl Counter",
                IMPL,
                (4, 7),
                5,
                vec![
                    a_symbol("peek", METHOD, (5, 5), 7, vec![]),
                    a_symbol("reset", METHOD, (6, 6), 18, vec![]),
                ]
            ),
            a_symbol(
                "impl Display for Counter",
                IMPL,
                (8, 10),
                5,
                vec![a_symbol("fmt", METHOD, (9, 9), 7, vec![])]
            ),
            a_symbol(
                "Shape",
                ENUM,
                (11, 13),
                9,
                vec![a_symbol("Round", VARIANT, (12, 12), 4, vec![])]
            ),
        ]);

        // When the members of everything on lines 1-14 are read
        let members = members_of(&outline, text, 1..=14, true);

        // Then only the private members of the struct and the inherent `impl` are surveyed
        assert_eq!(the_names_of(&members), ["Counter::count", "Counter::peek"]);
    }

    #[test]
    fn a_tuple_struct_contributes_the_fields_the_outline_lists() {
        // Given a tuple struct, whose outline symbol carries no field children
        // TODO(reshape-widen-same-crate): replace this outline with the one rust-analyzer returns
        // for `struct Id(u32);`, captured at milestone M3 (see the tuple-field todo of
        // `#reshape` 7).
        let text = "pub struct Id(u32);\n";
        let outline = json!([a_symbol("Id", STRUCT, (0, 0), 11, vec![])]);

        // When its members are read
        let members = members_of(&outline, text, 1..=1, true);

        // Then nothing is surveyed: a split tuple struct is left to the compile gate
        assert_eq!(the_names_of(&members), Vec::<String>::new());
    }
}

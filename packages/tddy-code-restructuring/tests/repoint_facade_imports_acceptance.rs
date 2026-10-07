//! `repoint_facade_imports` names what a file uses by the crate that defines it, with no
//! rust-analyzer.
//!
//! A module that is to move into another crate must write the paths it uses by their defining
//! crate, or the move presents an edge back to the crate it leaves. `tddy-session-lifecycle`'s
//! `lib.rs` re-exports modules of other crates (`pub use tddy_daemon_kernel::config;`), so files
//! wrote `crate::config::DaemonConfig`; twelve files and thirty lines were re-pointed by hand, and
//! four of them sat in a grouped `use`, which a text search does not find.
//!
//! The operation is text-only: the plan line carries an anchor and nothing else, and the answer is
//! the path survey the cross-crate moves already trust. So it is resolved here through a backend
//! over the fake language server, which is never asked a question, and the files are compared
//! byte for byte: an edit that reaches past the path shows.
//!
//! A `symbol` anchor names one file. An `items` anchor on a `mod` declaration names a module; once
//! resolved it is the range of that declaration, which is how these tests anchor a module.

mod facade_imports;
mod harness;

use facade_imports::{
    a_repoint_of_the_file, a_repoint_of_the_module, an_app_declaring, an_app_whose_a_rs_holds,
    an_operation, deep_checking, refusal_of, resolving, resolving_through, statically_checking,
    the_file_after, the_notes_of, ALL_DEPENDENCIES, A_RS,
};
use harness::checking_the_plan;
use tddy_code_restructuring::{FileEdit, Overlay, Plan, Position, Range, TextEdit, WorkspaceEdit};

/// A plan line for `repoint_facade_imports` over `file`, with `extra` fields written after the anchor.
fn a_plan_line_with(extra: &str) -> String {
    format!(
        "{{\"v\":1,\"snapshot\":{{}}}}\n{{\"op\":\"repoint_facade_imports\",\"anchor\":\
         {{\"kind\":\"symbol\",\"file\":\"app/src/a.rs\",\"path\":\"app\"}}{extra}}}\n"
    )
}

/// What reading the plan refuses with, or a failure saying it did not refuse.
fn refusal_of_reading(plan: &str) -> String {
    match Plan::parse(plan) {
        Err(refusal) => refusal.to_string(),
        Ok(_) => panic!("the plan was expected to be refused, and parsed:\n{plan}"),
    }
}

// ---------------------------------------------------------------------------------------------
// The plan line and the anchors
// ---------------------------------------------------------------------------------------------

#[test]
fn a_line_carrying_only_an_anchor_is_a_plan_and_every_other_field_is_refused() {
    // Given the fields a plan line can carry, each of which this operation has no use for
    let fields = [
        ("to", r#","to":"app::x""#),
        ("name", r#","name":"x""#),
        ("reexport", r#","reexport":"keep""#),
        ("variant", r#","variant":"first""#),
        ("type", r#","type":"u32""#),
        ("expr", r#","expr":"1""#),
        ("order", r#","order":[2,1]"#),
        ("also", r#","also":["app::y"]"#),
        ("to_file", r#","to_file":true"#),
        ("with_private_deps", r#","with_private_deps":true"#),
        ("callee", r#","callee":"f""#),
    ];

    // When a line carrying only its anchor is read, and each line with one more field
    let bare = Plan::parse(&a_plan_line_with(""));
    let refusals: Vec<(&str, String)> = fields
        .iter()
        .map(|(field, text)| (*field, refusal_of_reading(&a_plan_line_with(text))))
        .collect();

    // Then the bare line is a plan, and every other field is refused naming itself
    bare.expect("a line carrying only an anchor is a plan");
    for (field, refusal) in &refusals {
        assert!(
            refusal.contains(&format!("`{field}`")),
            "the refusal does not name `{field}`: {refusal}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_symbol_anchor_names_one_file_and_an_items_anchor_names_a_module_and_a_range_anchor_is_refused(
) {
    // Given a module with a file of its own, a sibling, and a facade through which each file reaches kernel
    let through_the_facade = "use crate::config::Settings;\n";
    let workspace = an_app_declaring(
        ALL_DEPENDENCIES,
        "",
        &format!("{through_the_facade}mod nested;\n"),
        &[("src/a/nested.rs", through_the_facade)],
    );

    // When the operation is anchored by symbol on the module's file, by the declaration of the module, and by a range over a statement
    let by_symbol = resolving(&workspace, a_repoint_of_the_file(A_RS)).await;
    let by_module = resolving(
        &workspace,
        a_repoint_of_the_module(&workspace, "app/src/lib.rs", "a"),
    )
    .await;
    let over_a_statement = refusal_of(
        &workspace,
        an_operation(serde_json::json!({
            "op": "repoint_facade_imports",
            "anchor": { "kind": "range", "file": A_RS,
                "start": { "line": 1, "col": 1 }, "end": { "line": 1, "col": 5 } },
        })),
    )
    .await;

    // Then the symbol names the one file, the declaration names both files of the module, and a range that is no module is refused
    let files_of = |resolved: Result<tddy_code_restructuring::Resolution, String>| -> Vec<String> {
        resolved
            .expect("the operation resolves")
            .edit
            .changes
            .into_iter()
            .filter_map(|change| match change {
                FileEdit::Change { path, .. } => Some(path),
                _ => None,
            })
            .collect()
    };
    assert_eq!(files_of(by_symbol), [A_RS]);
    assert_eq!(files_of(by_module), [A_RS, "app/src/a/nested.rs"]);
    assert!(
        over_a_statement.contains("module"),
        "a range over no `mod` declaration names no module, and the refusal should say so: {over_a_statement}"
    );
}

// ---------------------------------------------------------------------------------------------
// Which paths are re-pointed
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_plain_use_through_a_facade_is_re_pointed_to_the_crate_that_defines_it() {
    // Given a file importing through the facade
    let workspace = an_app_whose_a_rs_holds(
        "use crate::config::Limits;\n\npub struct Holder(pub Option<Limits>);\n",
    );

    // When it is re-pointed
    let after = the_file_after(&workspace, a_repoint_of_the_file(A_RS), A_RS).await;

    // Then the import names the crate that defines the item, and nothing else changed
    assert_eq!(
        after,
        "use kernel::config::Limits;\n\npub struct Holder(pub Option<Limits>);\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_path_in_a_body_is_re_pointed_and_the_rest_of_the_line_is_untouched() {
    // Given a body calling a function through the facade, with a trailing comment on the line
    let workspace = an_app_whose_a_rs_holds(
        "pub fn limit() -> u32 {\n    let l = crate::config::standard_limits(); // keep\n    l.max\n}\n",
    );

    // When it is re-pointed
    let after = the_file_after(&workspace, a_repoint_of_the_file(A_RS), A_RS).await;

    // Then only the path changed
    assert_eq!(
        after,
        "pub fn limit() -> u32 {\n    let l = kernel::config::standard_limits(); // keep\n    l.max\n}\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_glob_import_through_a_facade_is_re_pointed() {
    // Given a glob import of a module the crate re-exports
    let workspace = an_app_whose_a_rs_holds("use crate::config::*;\n");

    // When it is re-pointed
    let after = the_file_after(&workspace, a_repoint_of_the_file(A_RS), A_RS).await;

    // Then the glob is of the defining module
    assert_eq!(after, "use kernel::config::*;\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chain_of_facades_is_followed_to_the_defining_crate() {
    // Given an import through `app`'s facade of `mid`, which re-exports `kernel`, and one through a registry crate's re-export
    let workspace =
        an_app_whose_a_rs_holds("use crate::mid_config::Settings;\nuse crate::Clock;\n");

    // When it is re-pointed
    let after = the_file_after(&workspace, a_repoint_of_the_file(A_RS), A_RS).await;

    // Then each names its defining crate: the end of the chain, and the registry crate
    assert_eq!(
        after,
        "use kernel::config::Settings;\nuse regcrate::Clock;\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_explicit_use_of_a_registry_crate_is_re_pointed() {
    // Given a use of a registry crate's item through the facade's `pub use regcrate::Clock;`
    let workspace =
        an_app_whose_a_rs_holds("use crate::Clock;\n\npub fn now(c: Clock) -> Clock {\n    c\n}\n");

    // When it is re-pointed
    let after = the_file_after(&workspace, a_repoint_of_the_file(A_RS), A_RS).await;

    // Then the registry crate is named
    assert_eq!(
        after,
        "use regcrate::Clock;\n\npub fn now(c: Clock) -> Clock {\n    c\n}\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_path_to_an_item_the_crate_defines_itself_an_in_crate_facade_and_a_path_already_written_with_a_dependencys_name_are_left_alone(
) {
    // Given a file whose every path is the crate's own, an in-crate facade, or already foreign
    let own = concat!(
        "use crate::b::Thing;\n",
        "use crate::Thing as Inner;\n",
        "use kernel::config::Settings;\n",
        "use self::helpers::help;\n",
        "use super::b::Thing as Sibling;\n",
        "\n",
        "mod helpers {\n    pub fn help() {}\n}\n",
    );
    let workspace = an_app_whose_a_rs_holds(own);

    // When it is re-pointed
    let after = the_file_after(&workspace, a_repoint_of_the_file(A_RS), A_RS).await;

    // Then the file is byte for byte as it was
    assert_eq!(after, own);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_path_in_a_comment_a_doc_comment_or_a_string_is_left_alone() {
    // Given a file naming facade paths in a comment, a doc comment and a string, and once for real
    let workspace = an_app_whose_a_rs_holds(concat!(
        "use crate::config::Settings;\n",
        "// crate::config::Settings is only a comment\n",
        "/// See crate::config::Limits.\n",
        "pub fn text() -> &'static str {\n",
        "    \"crate::config::Settings\"\n",
        "}\n",
    ));

    // When it is re-pointed
    let after = the_file_after(&workspace, a_repoint_of_the_file(A_RS), A_RS).await;

    // Then only the real path changed, and the three lines that merely mention one are byte for byte
    assert_eq!(
        after,
        concat!(
            "use kernel::config::Settings;\n",
            "// crate::config::Settings is only a comment\n",
            "/// See crate::config::Limits.\n",
            "pub fn text() -> &'static str {\n",
            "    \"crate::config::Settings\"\n",
            "}\n",
        )
    );
}

// ---------------------------------------------------------------------------------------------
// What is refused
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_defining_crate_the_package_does_not_depend_on_is_refused_naming_the_path_and_the_crate_and_nothing_is_written(
) {
    // Given `app` declaring everything except `kernel`, and a file importing through the facade into it
    let before = "use crate::config::Settings;\n";
    let workspace = an_app_declaring(
        "agents = { path = \"../agents\" }\nmid = { path = \"../mid\" }\nregcrate = \"1\"\n",
        "",
        before,
        &[],
    );

    // When it is re-pointed
    let refusal = refusal_of(&workspace, a_repoint_of_the_file(A_RS)).await;

    // Then the refusal names the path, the crate and the file, and the file is as it was
    for named in ["crate::config::Settings", "kernel", "app/src/a.rs"] {
        assert!(
            refusal.contains(named),
            "the refusal does not name `{named}`: {refusal}"
        );
    }
    assert_eq!(workspace.read(A_RS), before);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dev_dependency_is_a_target_only_for_a_path_under_cfg_test() {
    // Given `app` declaring `kernel` as a dev-dependency only, a path under `#[cfg(test)]` and, in a second tree, one outside it
    let under_test = "#[cfg(test)]\nmod tests {\n    use crate::config::Limits;\n}\n";
    let outside_test = "use crate::config::Limits;\n";
    let only_dev =
        "agents = { path = \"../agents\" }\nmid = { path = \"../mid\" }\nregcrate = \"1\"\n";
    let dev = "kernel = { path = \"../kernel\" }\n";
    let in_a_test = an_app_declaring(only_dev, dev, under_test, &[]);
    let outside = an_app_declaring(only_dev, dev, outside_test, &[]);

    // When each is re-pointed
    let after = the_file_after(&in_a_test, a_repoint_of_the_file(A_RS), A_RS).await;
    let refusal = refusal_of(&outside, a_repoint_of_the_file(A_RS)).await;

    // Then the test path is re-pointed, and the path outside a test is refused naming the crate
    assert_eq!(
        after,
        "#[cfg(test)]\nmod tests {\n    use kernel::config::Limits;\n}\n"
    );
    assert!(
        refusal.contains("kernel"),
        "the refusal does not name the crate: {refusal}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_facade_that_renames_keeps_the_name_with_as_in_a_use_and_is_refused_in_a_body() {
    // Given `pub use kernel::config::Settings as AppSettings;`, imported by a file and, in a second tree, named in a body
    let in_a_use = an_app_whose_a_rs_holds("use crate::AppSettings;\n");
    let in_a_body = an_app_whose_a_rs_holds(
        "pub fn s() -> usize {\n    std::mem::size_of::<crate::AppSettings>()\n}\n",
    );

    // When each is re-pointed
    let after = the_file_after(&in_a_use, a_repoint_of_the_file(A_RS), A_RS).await;
    let refusal = refusal_of(&in_a_body, a_repoint_of_the_file(A_RS)).await;

    // Then the import keeps the name it was used by, and the body path is refused naming itself
    assert_eq!(after, "use kernel::config::Settings as AppSettings;\n");
    assert!(
        refusal.contains("crate::AppSettings"),
        "the refusal does not name the path: {refusal}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_path_spelled_across_whitespace_or_a_comment_is_refused_not_guessed() {
    // Given a path split over two lines, and in a second tree one with a comment inside it
    let across_lines = an_app_whose_a_rs_holds("use crate::\n    config::Settings;\n");
    let with_a_comment = an_app_whose_a_rs_holds("use crate::config /* facade */ ::Settings;\n");

    // When each is re-pointed
    let across = refusal_of(&across_lines, a_repoint_of_the_file(A_RS)).await;
    let commented = refusal_of(&with_a_comment, a_repoint_of_the_file(A_RS)).await;

    // Then both are refused naming the file and the line, and neither file is written
    for refusal in [&across, &commented] {
        assert!(
            refusal.contains("app/src/a.rs:1"),
            "the refusal does not name the file and line: {refusal}"
        );
    }
    assert_eq!(
        across_lines.read(A_RS),
        "use crate::\n    config::Settings;\n"
    );
    assert_eq!(
        with_a_comment.read(A_RS),
        "use crate::config /* facade */ ::Settings;\n"
    );
}

// ---------------------------------------------------------------------------------------------
// Grouped `use`
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_group_whose_members_agree_gets_its_prefix_re_pointed_in_place() {
    // Given two groups whose members all go through the same facade
    let workspace = an_app_whose_a_rs_holds(
        "use crate::config::{self, Settings};\nuse crate::{config::Limits, config::standard_limits};\n",
    );

    // When it is re-pointed
    let after = the_file_after(&workspace, a_repoint_of_the_file(A_RS), A_RS).await;

    // Then each group keeps its shape under the defining crate's name
    assert_eq!(
        after,
        "use kernel::config::{self, Settings};\nuse kernel::{config::Limits, config::standard_limits};\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_group_whose_members_disagree_is_split_kept_members_first_then_one_use_per_lifted_member()
{
    // Given the group of the todo: one member through the facade, one the crate's own
    let workspace = an_app_whose_a_rs_holds("use crate::{config::Limits, b::Thing};\n");

    // When it is re-pointed
    let after = the_file_after(&workspace, a_repoint_of_the_file(A_RS), A_RS).await;

    // Then the kept member stays under the original prefix, and the lifted one follows on its own line (rustfmt, which apply runs, writes the first without braces)
    assert_eq!(
        after,
        "use crate::{b::Thing};\nuse kernel::config::Limits;\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_group_whose_every_member_goes_through_a_facade_becomes_one_use_per_member() {
    // Given a group whose members are re-exported from two different crates
    let workspace = an_app_whose_a_rs_holds("use crate::{config::Limits, user_paths::home};\n");

    // When it is re-pointed
    let after = the_file_after(&workspace, a_repoint_of_the_file(A_RS), A_RS).await;

    // Then the group disappears into one statement per member, in member order
    assert_eq!(
        after,
        "use kernel::config::Limits;\nuse kernel::paths::home;\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_nested_group_member_is_lifted_whole_when_its_leaves_agree_and_refused_when_they_do_not()
{
    // Given a nested group whose leaves share one new prefix, and in a second tree one whose leaves go to two crates
    let agreeing = an_app_whose_a_rs_holds("use crate::{config::{Limits, Settings}, b::Thing};\n");
    let disagreeing = an_app_whose_a_rs_holds("use crate::{mix::{Limits, Roster}, b::Thing};\n");

    // When each is re-pointed
    let after = the_file_after(&agreeing, a_repoint_of_the_file(A_RS), A_RS).await;
    let refusal = refusal_of(&disagreeing, a_repoint_of_the_file(A_RS)).await;

    // Then the first lifts the nested group whole, and the second is refused naming the member
    assert_eq!(
        after,
        "use crate::{b::Thing};\nuse kernel::config::{Limits, Settings};\n"
    );
    assert!(
        refusal.contains("mix"),
        "the refusal does not name the member: {refusal}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_attribute_or_doc_comment_above_a_group_that_would_split_is_refused_and_above_a_plain_use_is_kept(
) {
    // Given a gated group that must split, a documented one, and a gated plain use
    let gated_group =
        an_app_whose_a_rs_holds("#[cfg(unix)]\nuse crate::{config::Limits, b::Thing};\n");
    let documented_group =
        an_app_whose_a_rs_holds("/// The limits.\nuse crate::{config::Limits, b::Thing};\n");
    let gated_plain = an_app_whose_a_rs_holds("#[cfg(unix)]\nuse crate::config::Limits;\n");

    // When each is re-pointed
    let gated = refusal_of(&gated_group, a_repoint_of_the_file(A_RS)).await;
    let documented = refusal_of(&documented_group, a_repoint_of_the_file(A_RS)).await;
    let kept = the_file_after(&gated_plain, a_repoint_of_the_file(A_RS), A_RS).await;

    // Then the groups are refused naming the file and line, and the plain use is re-pointed under its attribute
    for refusal in [&gated, &documented] {
        assert!(
            refusal.contains("app/src/a.rs:2"),
            "the refusal does not name the file and the line of the statement: {refusal}"
        );
    }
    assert_eq!(kept, "#[cfg(unix)]\nuse kernel::config::Limits;\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rewrite_that_would_bind_a_name_the_scope_already_binds_is_refused_naming_both() {
    // Given a file that already imports the item from its defining crate, and again through the facade
    let workspace =
        an_app_whose_a_rs_holds("use kernel::config::Settings;\nuse crate::config::Settings;\n");

    // When it is re-pointed
    let refusal = refusal_of(&workspace, a_repoint_of_the_file(A_RS)).await;

    // Then the refusal names both paths, and the file is as it was
    for named in ["kernel::config::Settings", "crate::config::Settings"] {
        assert!(
            refusal.contains(named),
            "the refusal does not name `{named}`: {refusal}"
        );
    }
    assert_eq!(
        workspace.read(A_RS),
        "use kernel::config::Settings;\nuse crate::config::Settings;\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_split_keeps_the_visibility_and_the_indentation_of_the_statement_it_replaced() {
    // Given a `pub use` group and, inside a module, an indented group
    let workspace = an_app_whose_a_rs_holds(concat!(
        "pub use crate::{config::Limits, b::Thing};\n",
        "\n",
        "mod tests {\n",
        "    use crate::{config::Settings, b::Thing as Other};\n",
        "}\n",
    ));

    // When it is re-pointed
    let after = the_file_after(&workspace, a_repoint_of_the_file(A_RS), A_RS).await;

    // Then each lifted statement carries the visibility and the indentation of its group
    assert_eq!(
        after,
        concat!(
            "pub use crate::{b::Thing};\n",
            "pub use kernel::config::Limits;\n",
            "\n",
            "mod tests {\n",
            "    use crate::{b::Thing as Other};\n",
            "    use kernel::config::Settings;\n",
            "}\n",
        )
    );
}

// ---------------------------------------------------------------------------------------------
// Idempotence, scope, parity
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn running_the_operation_on_its_own_output_rewrites_nothing_and_says_so() {
    // Given a file already re-pointed by the operation
    let workspace = an_app_whose_a_rs_holds("use crate::{config::Limits, b::Thing};\n");
    the_file_after(&workspace, a_repoint_of_the_file(A_RS), A_RS).await;

    // When it is run again
    let second = resolving(&workspace, a_repoint_of_the_file(A_RS))
        .await
        .expect("the second run resolves");

    // Then it edits nothing, and its note says nothing in the file goes through a facade
    let edited: Vec<&FileEdit> = second
        .edit
        .changes
        .iter()
        .filter(|change| matches!(change, FileEdit::Change { edits, .. } if !edits.is_empty()))
        .collect();
    assert!(edited.is_empty(), "the second run edited: {edited:?}");
    assert!(
        second
            .notes
            .iter()
            .any(|note| note
                .contains("nothing in app/src/a.rs goes through a facade of another crate")),
        "the notes do not say nothing was left to re-point: {:?}",
        second.notes
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_module_anchor_covers_every_file_of_the_module_and_a_file_anchor_only_its_own() {
    // Given a module of two files and a sibling, each importing through the facade
    let through_the_facade = "use crate::config::Settings;\n";
    let workspace = an_app_declaring(
        ALL_DEPENDENCIES,
        "",
        &format!("{through_the_facade}mod nested;\n"),
        &[
            ("src/a/nested.rs", through_the_facade),
            ("src/b.rs", through_the_facade),
        ],
    );

    // When the module is re-pointed, and then (in a fresh tree) the file alone
    let module = the_file_after(
        &workspace,
        a_repoint_of_the_module(&workspace, "app/src/lib.rs", "a"),
        "app/src/a.rs",
    )
    .await;
    let nested = workspace.read("app/src/a/nested.rs");
    let sibling = workspace.read("app/src/b.rs");
    let alone = an_app_declaring(
        ALL_DEPENDENCIES,
        "",
        &format!("{through_the_facade}mod nested;\n"),
        &[
            ("src/a/nested.rs", through_the_facade),
            ("src/b.rs", through_the_facade),
        ],
    );
    the_file_after(&alone, a_repoint_of_the_file(A_RS), A_RS).await;

    // Then the module's files are re-pointed and the sibling is not, while the file anchor leaves the module's other file alone
    assert_eq!(module, "use kernel::config::Settings;\nmod nested;\n");
    assert_eq!(nested, "use kernel::config::Settings;\n");
    assert_eq!(sibling, through_the_facade);
    assert_eq!(alone.read("app/src/a/nested.rs"), through_the_facade);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_static_check_of_a_file_anchored_plan_reports_the_refusals_resolve_reports() {
    // Given the four refusals of this operation that the text of one file decides
    let undeclared = an_app_declaring(
        "agents = { path = \"../agents\" }\nmid = { path = \"../mid\" }\nregcrate = \"1\"\n",
        "",
        "use crate::config::Settings;\n",
        &[],
    );
    let renamed = an_app_whose_a_rs_holds(
        "pub fn s() -> usize {\n    std::mem::size_of::<crate::AppSettings>()\n}\n",
    );
    let spelled_across = an_app_whose_a_rs_holds("use crate::\n    config::Settings;\n");
    let binding_twice =
        an_app_whose_a_rs_holds("use kernel::config::Settings;\nuse crate::config::Settings;\n");
    let cases = [
        (&undeclared, "crate::config::Settings"),
        (&renamed, "crate::AppSettings"),
        (&spelled_across, "app/src/a.rs:1"),
        (&binding_twice, "kernel::config::Settings"),
    ];

    // When each is checked without a server
    let mut checked = Vec::new();
    for (workspace, _) in &cases {
        checked.push(
            statically_checking(workspace, a_repoint_of_the_file(A_RS))
                .await
                .expect("a static check runs"),
        );
    }

    // Then every check reports a finding naming what the refusal of the resolution names
    for ((_, named), findings) in cases.iter().zip(&checked) {
        assert!(
            findings.iter().any(|finding| finding.contains(named)),
            "the static check did not report the refusal naming `{named}`: {findings:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_static_check_of_a_module_anchored_plan_says_to_run_deep() {
    // Given a plan anchoring the operation by item on a module's declaration
    let workspace = an_app_whose_a_rs_holds("use crate::config::Settings;\n");
    let plan = workspace.a_plan_of(&[an_operation(serde_json::json!({
        "op": "repoint_facade_imports",
        "anchor": { "kind": "items", "file": "app/src/lib.rs",
            "items": ["app::a"], "fingerprints": ["sha256:written"] },
    }))]);

    // When it is checked without a server
    let findings = checking_the_plan(&workspace, plan, false)
        .await
        .expect("a static check runs");

    // Then the one finding says the operation was not examined and names the way to examine it
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(
        findings[0].contains("RepointFacadeImports") && findings[0].contains("check --deep"),
        "the finding does not name the operation and `check --deep`: {}",
        findings[0]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_edits_are_the_same_whether_the_file_was_read_from_disk_or_through_an_earlier_operations_overlay(
) {
    // Given a file importing through the facade, and an earlier operation's edit that put a line above it
    let workspace = an_app_whose_a_rs_holds("use crate::config::Settings;\n");
    let mut overlay = Overlay::new();
    overlay
        .record(
            workspace.path(),
            &WorkspaceEdit {
                changes: vec![FileEdit::Change {
                    path: A_RS.to_string(),
                    edits: vec![TextEdit {
                        range: Range {
                            start: Position { line: 1, col: 1 },
                            end: Position { line: 1, col: 1 },
                        },
                        new_text: "// written by an earlier operation\n".to_string(),
                    }],
                }],
            },
        )
        .expect("the earlier edit is recorded");

    // When the operation is resolved against the disk and against the overlay
    let from_disk = resolving(&workspace, a_repoint_of_the_file(A_RS)).await;
    let through_the_overlay =
        resolving_through(&workspace, a_repoint_of_the_file(A_RS), overlay).await;

    // Then the same replacement is made, one line lower in the second
    let edit_of = |resolved: Result<tddy_code_restructuring::Resolution, String>| match resolved
        .expect("the operation resolves")
        .edit
        .changes
        .remove(0)
    {
        FileEdit::Change { mut edits, .. } => edits.remove(0),
        other => panic!("expected a text change, got {other:?}"),
    };
    let (disk, overlaid) = (edit_of(from_disk), edit_of(through_the_overlay));
    assert_eq!(disk.new_text, overlaid.new_text);
    assert_eq!(overlaid.range.start.line, disk.range.start.line + 1);
}

// ---------------------------------------------------------------------------------------------
// The account `check --deep` and `apply` give
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_deep_check_note_lists_every_path_it_would_rewrite_with_file_line_written_and_defined() {
    // Given a file with a plain use, a group that splits, and a body path
    let workspace = an_app_whose_a_rs_holds(concat!(
        "use crate::config::Settings;\n",
        "use crate::{config::Limits, b::Thing};\n",
        "\n",
        "pub fn limit() -> u32 {\n",
        "    crate::config::standard_limits().max\n",
        "}\n",
    ));
    let plan = workspace.a_plan_of(&[a_repoint_of_the_file(A_RS)]);

    // When the plan is deep-checked
    let (findings, account) = deep_checking(&workspace, plan).await;

    // Then every path is a note, in source order, and none of it is a finding
    assert_eq!(findings.expect("the check runs"), Vec::<String>::new());
    assert_eq!(
        the_notes_of(&account),
        [
            "   note: repoint_facade_imports: 3 path(s) in 1 file(s) go through a facade of another crate",
            "   note:   app/src/a.rs:1: crate::config::Settings -> kernel::config::Settings",
            "   note:   app/src/a.rs:2: crate::config::Limits -> kernel::config::Limits (split out of a grouped `use`)",
            "   note:   app/src/a.rs:5: crate::config::standard_limits -> kernel::config::standard_limits",
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deep_check_note_for_a_clean_file_says_nothing_goes_through_a_facade_and_is_not_a_finding(
) {
    // Given a file naming only its own items
    let workspace = an_app_whose_a_rs_holds("use crate::b::Thing;\n");
    let plan = workspace.a_plan_of(&[a_repoint_of_the_file(A_RS)]);

    // When the plan is deep-checked
    let (findings, account) = deep_checking(&workspace, plan).await;

    // Then the note says so, and the check has no finding
    assert_eq!(findings.expect("the check runs"), Vec::<String>::new());
    assert_eq!(
        the_notes_of(&account),
        ["   note: repoint_facade_imports: nothing in app/src/a.rs goes through a facade of another crate"]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deep_check_forwards_the_notes_of_every_operation_and_changes_no_finding() {
    // Given a plan of two operations, over two files, each with a path through the facade
    let workspace = an_app_declaring(
        ALL_DEPENDENCIES,
        "",
        "use crate::config::Settings;\n",
        &[("src/b.rs", "use crate::config::Limits;\n")],
    );
    let plan = workspace.a_plan_of(&[
        a_repoint_of_the_file(A_RS),
        a_repoint_of_the_file("app/src/b.rs"),
    ]);

    // When the plan is deep-checked
    let (findings, account) = deep_checking(&workspace, plan).await;

    // Then both operations' notes are in the account, in operation order, and there is no finding
    assert_eq!(findings.expect("the check runs"), Vec::<String>::new());
    assert_eq!(
        the_notes_of(&account),
        [
            "   note: repoint_facade_imports: 1 path(s) in 1 file(s) go through a facade of another crate",
            "   note:   app/src/a.rs:1: crate::config::Settings -> kernel::config::Settings",
            "   note: repoint_facade_imports: 1 path(s) in 1 file(s) go through a facade of another crate",
            "   note:   app/src/b.rs:1: crate::config::Limits -> kernel::config::Limits",
        ]
    );
}

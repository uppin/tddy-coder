//! What `extract_method` leaves behind, against a live rust-analyzer: the range's comments in
//! place, a signature CI's lint job accepts, and types named the way the file names them.
//!
//! Every fixture is a shape #524's lifecycle destructure fixed by hand, reproduced against the dev
//! shell's rust-analyzer (2026-03-30) while planning `#reshape` 4/19. `extract_module`'s carry of
//! a parent's `use Trait as _;` is here too: it is the same import gap, seen from the other
//! extraction.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use std::time::{Duration, Instant};

use harness::{
    a_workspace_holding_files, an_extract_method_at, an_extract_method_of, an_extract_module_of,
    applying_a_plan_of, applying_keeping_the_account, assert_compiles, assert_lints_clean, at,
    performing, resolving, the_function_named, the_module_named, ORIGIN_LIB,
};
use tddy_code_restructuring::Anchor;

/// A ready index answers in seconds; far under the harness's three-minute ceiling, and over the
/// 30-second bound a probe that can never be typed is refused at.
const A_READY_INDEX_ANSWERS_WITHIN: Duration = Duration::from_secs(60);

/// A one-crate workspace whose library is `lines`.
fn one_crate_holding(lines: &[&str]) -> harness::AFixtureWorkspace {
    a_workspace_holding_files(&[
        (
            "Cargo.toml",
            "[workspace]\nresolver = \"2\"\nmembers = [\"crates/origin\"]\n",
        ),
        (
            "crates/origin/Cargo.toml",
            "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (ORIGIN_LIB, &(lines.join("\n") + "\n")),
    ])
}

/// `start`'s first statements, as #524's plan `09b` cut `warm_up_jail_agents`: lines 11–20 hold
/// a `?`, two full-line comments, one comment inside a method chain and one trailing comment.
fn a_service_whose_start_explains_itself() -> harness::AFixtureWorkspace {
    one_crate_holding(&[
        "pub struct Svc {",
        "    roster: Vec<String>,",
        "}",
        "",
        "impl Svc {",
        "    async fn seeded(&self, n: &[String]) -> Result<Vec<String>, String> {",
        "        Ok(n.to_vec())",
        "    }",
        "",
        "    pub async fn start(&self, agents: &[String], dir: std::path::PathBuf) -> Result<usize, String> {",
        "        let mut started = self.seeded(agents).await?;",
        "        // The defs behind those records, which the jail env can only carry for agents this host",
        "        // holds — the records above are what carries the rest.",
        "        let defs = self",
        "            .roster",
        "            .iter()",
        "            // only the held ones",
        "            .filter(|r| agents.contains(r))",
        "            .count();",
        "        let p = dir.join(\"x\"); // trailing note",
        "        started.push(p.display().to_string());",
        "        Ok(started.len() + defs)",
        "    }",
        "}",
    ])
}

#[tokio::test(flavor = "multi_thread")]
async fn comments_between_the_statements_of_a_range_holding_a_question_mark_stay_beside_their_statements(
) {
    // Given a range holding `.await?` and two comment lines between its statements
    let workspace = a_service_whose_start_explains_itself();

    // When it is extracted
    performing(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 11..=20, "warm_up"),
    )
    .await;

    // Then both comment lines sit before the statement they annotated, in the new function
    let warm_up = the_function_named(&workspace.read(ORIGIN_LIB), "warm_up");
    assert!(
        warm_up.contains(
            "        let mut started = self.seeded(agents).await?;\n        \
             // The defs behind those records, which the jail env can only carry for agents this host\n        \
             // holds — the records above are what carries the rest.\n        let defs = self\n"
        ),
        "the comments did not stay beside `let defs`:\n{warm_up}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_trailing_comment_stays_at_the_end_of_its_statement() {
    // Given the same range, whose last statement carries a trailing comment
    let workspace = a_service_whose_start_explains_itself();

    // When it is extracted
    performing(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 11..=20, "warm_up"),
    )
    .await;

    // Then the comment still ends that statement's line
    let warm_up = the_function_named(&workspace.read(ORIGIN_LIB), "warm_up");
    assert!(
        warm_up.contains("        let p = dir.join(\"x\"); // trailing note\n"),
        "the trailing comment was lost:\n{warm_up}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_comment_inside_a_statement_is_kept_once() {
    // Given the same range, whose method chain holds a comment the assist keeps by itself
    let workspace = a_service_whose_start_explains_itself();

    // When it is extracted
    performing(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 11..=20, "warm_up"),
    )
    .await;

    // Then that comment appears exactly once, and the dropped ones are back
    let lib = workspace.read(ORIGIN_LIB);
    assert_eq!(lib.matches("// only the held ones").count(), 1, "{lib}");
    assert_eq!(lib.matches("// trailing note").count(), 1, "{lib}");
}

/// Lines 4–5 borrow three owned locals the caller goes on using.
fn a_function_that_borrows_its_owned_locals() -> harness::AFixtureWorkspace {
    one_crate_holding(&[
        "use std::path::PathBuf;",
        "",
        "pub fn joined(dir: PathBuf, label: String, items: Vec<u32>) -> usize {",
        "    let copy = dir.clone();",
        "    let joined = copy.join(&label).join(items.len().to_string());",
        "    joined.as_os_str().len() + dir.as_os_str().len() + label.len() + items.len()",
        "}",
    ])
}

#[tokio::test(flavor = "multi_thread")]
async fn borrowed_path_string_and_vec_locals_are_taken_as_path_str_and_slice_and_the_tree_compiles()
{
    // Given a range borrowing a `PathBuf`, a `String` and a `Vec<u32>`
    let workspace = a_function_that_borrows_its_owned_locals();

    // When it is extracted
    performing(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 4..=5, "kept_joined"),
    )
    .await;

    // Then the new function takes the slices, `Path` spelled in full where the file binds only `PathBuf`
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("fn kept_joined(dir: &std::path::Path, label: &str, items: &[u32])"),
        "the parameters were not narrowed:\n{lib}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_borrowed_path_the_range_clones_becomes_an_owned_copy_of_the_path() {
    // Given a range that clones the path it borrows
    let workspace = a_function_that_borrows_its_owned_locals();

    // When it is extracted
    performing(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 4..=5, "kept_joined"),
    )
    .await;

    // Then the clone of the narrowed `&Path` is an owned copy, and the tree compiles
    let kept_joined = the_function_named(&workspace.read(ORIGIN_LIB), "kept_joined");
    assert!(
        kept_joined.contains("let copy = dir.to_owned();"),
        "the clone was not made an owned copy:\n{kept_joined}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_parameter_the_body_passes_where_a_path_buf_is_needed_keeps_its_type_and_says_so() {
    // Given a range passing its borrowed `PathBuf` to a function that takes `&PathBuf`
    let workspace = one_crate_holding(&[
        "use std::path::PathBuf;",
        "",
        "#[allow(clippy::ptr_arg)]",
        "fn needs_buf(p: &PathBuf) -> usize {",
        "    p.as_os_str().len()",
        "}",
        "",
        "pub fn measured(dir: PathBuf) -> usize {",
        "    let n = needs_buf(&dir);",
        "    n + dir.as_os_str().len()",
        "}",
    ]);

    // When it is extracted
    let (applied, account) = applying_keeping_the_account(
        &workspace,
        &[an_extract_method_of(
            &workspace,
            ORIGIN_LIB,
            9..=9,
            "measured_buf",
        )],
        false,
    )
    .await;

    // Then the parameter keeps rust-analyzer's type, the run says why, and the tree compiles
    assert!(applied.is_ok(), "the apply failed: {applied:?}");
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("fn measured_buf(dir: &PathBuf) -> usize {"),
        "the parameter was narrowed:\n{lib}"
    );
    assert!(
        account.iter().any(|line| line
            .contains("kept `dir: &PathBuf`: the body needs the type rust-analyzer wrote")),
        "the run did not say the parameter was kept:\n{account:#?}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_unit_body_holding_a_question_mark_ends_in_ok_unit_rather_than_wrapping_the_if() {
    // Given a unit `if` holding a `?`, in a function returning `Result<(), String>`
    let workspace = one_crate_holding(&[
        "use std::path::PathBuf;",
        "",
        "pub fn unit_tail(dir: PathBuf, flag: bool) -> Result<(), String> {",
        "    let n = dir.as_os_str().len();",
        "    if flag {",
        "        let _p = dir.join(\"a\");",
        "        let _x = n + 1;",
        "        Err::<(), String>(\"e\".into())?;",
        "    }",
        "    let _q = dir.join(\"b\");",
        "    Ok(())",
        "}",
    ]);

    // When the `if` is extracted
    performing(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 5..=9, "guarded_join"),
    )
    .await;

    // Then the new function runs the `if` as a statement and ends in `Ok(())`
    let guarded_join = the_function_named(&workspace.read(ORIGIN_LIB), "guarded_join");
    assert!(
        !guarded_join.contains("Ok(if"),
        "the unit `if` is still wrapped:\n{guarded_join}"
    );
    assert!(
        guarded_join.ends_with("    }\n    Ok(())\n}"),
        "the function does not end in `Ok(())`:\n{guarded_join}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_field_initialised_from_a_parameter_of_its_own_name_is_written_in_shorthand() {
    // Given a struct literal whose fields borrow locals of the same names
    let workspace = one_crate_holding(&[
        "use std::path::PathBuf;",
        "",
        "pub struct Lookup<'a> {",
        "    pub base: &'a PathBuf,",
        "    pub root: &'a PathBuf,",
        "}",
        "",
        "fn consume(l: &Lookup<'_>) -> usize {",
        "    l.base.as_os_str().len() + l.root.as_os_str().len()",
        "}",
        "",
        "pub fn field_names(base: PathBuf, root: PathBuf) -> usize {",
        "    let n = consume(&Lookup { base: &base, root: &root });",
        "    n + base.as_os_str().len() + root.as_os_str().len()",
        "}",
    ]);

    // When the line building it is extracted
    performing(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 13..=13, "looked_up"),
    )
    .await;

    // Then the fields are written in shorthand, and the tree compiles
    let looked_up = the_function_named(&workspace.read(ORIGIN_LIB), "looked_up");
    assert!(
        looked_up.contains("consume(&Lookup { base, root })"),
        "the fields are not in shorthand:\n{looked_up}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_extraction_taking_more_than_seven_parameters_says_so_in_a_note() {
    // Given a statement reading eight locals
    let workspace = one_crate_holding(&[
        "pub fn wide(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8, g: u8, h: u8) -> u8 {",
        "    let sum = a + b + c + d + e + f + g + h;",
        "    sum / 2",
        "}",
    ]);

    // When it is extracted in a dry run
    let (_, account) = applying_keeping_the_account(
        &workspace,
        &[an_extract_method_of(
            &workspace,
            ORIGIN_LIB,
            2..=2,
            "summed",
        )],
        true,
    )
    .await;

    // Then the run says the new function's arity is the plan's to fix
    assert!(
        account.iter().any(|line| line
            .contains("`summed` takes 8 parameters; clippy::too_many_arguments fires above 7")),
        "the arity was not noted:\n{account:#?}"
    );
}

/// `recipe` spells the trait `deep::recipe::WorkflowRecipe`, and nothing imports it.
fn a_function_naming_a_trait_qualified(second_spelling: bool) -> harness::AFixtureWorkspace {
    let header = if second_spelling {
        "pub fn recipe(r: Option<std::sync::Arc<dyn deep::recipe::WorkflowRecipe>>, s: &dyn crate::deep::recipe::WorkflowRecipe) -> usize {"
    } else {
        "pub fn recipe(r: Option<std::sync::Arc<dyn deep::recipe::WorkflowRecipe>>) -> usize {"
    };
    let tail = if second_spelling {
        "    name.len() + s.name().len()"
    } else {
        "    name.len()"
    };
    one_crate_holding(&[
        "pub mod deep {",
        "    pub mod recipe {",
        "        pub trait WorkflowRecipe {",
        "            fn name(&self) -> String;",
        "        }",
        "    }",
        "}",
        "",
        header,
        "    let name = r.as_ref().map(|r| r.name()).unwrap_or_default();",
        tail,
        "}",
    ])
}

#[tokio::test(flavor = "multi_thread")]
async fn a_trait_the_origin_spells_qualified_is_spelled_the_same_way_in_the_new_signature_and_compiles(
) {
    // Given a range reading a parameter whose trait the file spells qualified
    let workspace = a_function_naming_a_trait_qualified(false);

    // When it is extracted
    performing(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 10..=10, "recipe_name"),
    )
    .await;

    // Then the new signature spells it the same way, no `use` is added, and the tree compiles
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        the_function_named(&lib, "recipe_name")
            .contains("dyn deep::recipe::WorkflowRecipe + 'static"),
        "the trait is not spelled as the origin spells it:\n{lib}"
    );
    assert!(!lib.contains("use "), "a `use` was added:\n{lib}");
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_type_the_origin_spells_two_ways_is_refused_naming_both_spellings_and_nothing_is_written()
{
    // Given a function spelling the trait two ways
    let workspace = a_function_naming_a_trait_qualified(true);
    let before = workspace.read(ORIGIN_LIB);

    // When the range naming it is extracted
    let refused = resolving(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 10..=10, "recipe_name"),
    )
    .await;

    // Then it is refused, naming both spellings, and the file is unchanged
    assert_eq!(
        refused.expect_err("two spellings are refused"),
        "this seam cannot be cut here: the extracted signature names `WorkflowRecipe`, which this \
         file does not import, and the function it came from spells it as \
         `deep::recipe::WorkflowRecipe` and `crate::deep::recipe::WorkflowRecipe`; import it or \
         cut the range elsewhere"
    );
    assert_eq!(workspace.read(ORIGIN_LIB), before);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_range_opening_on_a_blocks_brace_is_extracted_within_the_ready_bound() {
    // Given a bare block statement
    let workspace = one_crate_holding(&[
        "pub fn block(x: u32) -> u32 {",
        "    let mut y = x;",
        "    {",
        "        let z = y + 1;",
        "        y = z * 2;",
        "    }",
        "    y",
        "}",
    ]);
    let started = Instant::now();

    // When the range starting on its `{` is extracted
    let resolved = resolving(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 3..=6, "stepped"),
    )
    .await;

    // Then it resolves, within the time a ready index takes
    assert!(resolved.is_ok(), "the extraction was refused: {resolved:?}");
    assert!(
        started.elapsed() < A_READY_INDEX_ANSWERS_WITHIN,
        "the extraction took {:?}",
        started.elapsed()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_range_opening_on_a_line_comment_is_extracted_within_the_ready_bound_and_keeps_the_comment(
) {
    // Given a statement with a comment above it
    let workspace = one_crate_holding(&[
        "pub fn commented(x: u32) -> u32 {",
        "    let y = x + 1;",
        "    // the doubling step",
        "    let z = y * 2;",
        "    z + y",
        "}",
    ]);
    let started = Instant::now();

    // When the range starting on the comment is extracted
    performing(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 3..=4, "doubled"),
    )
    .await;

    // Then it lands within the ready bound, with the comment, and compiles
    assert!(
        started.elapsed() < A_READY_INDEX_ANSWERS_WITHIN,
        "the extraction took {:?}",
        started.elapsed()
    );
    assert!(
        the_function_named(&workspace.read(ORIGIN_LIB), "doubled").contains("// the doubling step"),
        "the comment was lost:\n{}",
        workspace.read(ORIGIN_LIB)
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_range_that_initialises_a_let_is_extracted_under_the_plans_name() {
    // Given a `let` whose initializer is an `if … else`
    let workspace = one_crate_holding(&[
        "use std::path::PathBuf;",
        "",
        "pub fn let_value(flag: bool, dir: PathBuf) -> usize {",
        "    let managed = if flag {",
        "        Some(dir.join(\"a\"))",
        "    } else {",
        "        None",
        "    };",
        "    managed.map(|p| p.as_os_str().len()).unwrap_or(0) + dir.as_os_str().len()",
        "}",
    ]);
    let the_initializer = Anchor::Range {
        file: ORIGIN_LIB.to_string(),
        start: at(4, 19),
        end: at(8, 6),
    };

    // When the initializer is extracted
    performing(
        &workspace,
        an_extract_method_at(the_initializer, "managed_recipe_for"),
    )
    .await;

    // Then the function rust-analyzer named after the binding carries the plan's name
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("let managed = managed_recipe_for(flag, &dir);"),
        "the call does not carry the plan's name:\n{lib}"
    );
    assert!(
        lib.contains("fn managed_recipe_for("),
        "the function does not carry the plan's name:\n{lib}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_extraction_shaped_like_plan_09b_passes_clippy_with_warnings_denied() {
    // Given #524's plan-09b shape: `.await?`, two borrowed `PathBuf`s, a copied `mut`, two comments
    let workspace = one_crate_holding(&[
        "use std::path::PathBuf;",
        "",
        "pub struct Svc {",
        "    roster: Vec<String>,",
        "}",
        "",
        "impl Svc {",
        "    async fn seeded(&self, n: &[String]) -> Result<Vec<String>, String> {",
        "        Ok(n.to_vec())",
        "    }",
        "",
        "    pub async fn start(&self, agents: &[String], dir: PathBuf, home: PathBuf) -> Result<usize, String> {",
        "        let mut started = self.seeded(agents).await?;",
        "        // The records above carry the rest.",
        "        let defs = self.roster.iter().filter(|r| agents.contains(r)).count();",
        "        // The jail's scratch home sits beside the session.",
        "        let scratch = dir.join(&home);",
        "        started.push(scratch.display().to_string());",
        "        Ok(started.len() + defs + dir.as_os_str().len() + home.as_os_str().len())",
        "    }",
        "}",
    ]);

    // When the range is applied through the runner, tidy included
    let applied = applying_a_plan_of(
        &workspace,
        &[an_extract_method_of(
            &workspace,
            ORIGIN_LIB,
            13..=17,
            "warm_up",
        )],
    )
    .await;

    // Then the run succeeded, kept both comments, and the workspace lints clean
    assert!(applied.is_ok(), "the apply failed: {applied:?}");
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("// The records above carry the rest.")
            && lib.contains("// The jail's scratch home sits beside the session."),
        "a comment was lost:\n{lib}"
    );
    assert_lints_clean(&workspace);
}

/// A crate whose root imports a trait `as _` for its method alone. Lines 14–16 call the method;
/// lines 18–20 call nothing of the trait's.
fn a_crate_whose_parent_imports_a_trait_for_its_method() -> harness::AFixtureWorkspace {
    one_crate_holding(&[
        "pub mod wire {",
        "    pub trait Encode {",
        "        fn encoded(&self) -> Vec<u8> {",
        "            vec![1]",
        "        }",
        "    }",
        "    pub struct Frame;",
        "    impl Encode for Frame {}",
        "}",
        "",
        "use crate::wire::Encode as _;",
        "use crate::wire::Frame;",
        "",
        "pub fn framed() -> usize {",
        "    Frame.encoded().len()",
        "}",
        "",
        "pub fn plain() -> usize {",
        "    2",
        "}",
    ])
}

#[tokio::test(flavor = "multi_thread")]
async fn an_extract_module_whose_seam_calls_a_trait_method_the_parent_imports_as_underscore_compiles(
) {
    // Given a seam calling a method of a trait the parent imports `as _`
    let workspace = a_crate_whose_parent_imports_a_trait_for_its_method();

    // When the seam becomes a module
    performing(
        &workspace,
        an_extract_module_of(&workspace, ORIGIN_LIB, 14..=16, "framing"),
    )
    .await;

    // Then the module carries the import, and the tree compiles
    let framing = the_module_named(&workspace.read(ORIGIN_LIB), "framing");
    assert!(
        framing.contains("use crate::wire::Encode as _;"),
        "the trait import was not carried:\n{framing}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_apply_leaves_no_unused_copy_of_the_carried_trait_import() {
    // Given the same crate, cut into two seams — one calls the trait's method, one calls nothing
    let workspace = a_crate_whose_parent_imports_a_trait_for_its_method();
    let seams = [
        an_extract_module_of(&workspace, ORIGIN_LIB, 18..=20, "plainly"),
        an_extract_module_of(&workspace, ORIGIN_LIB, 14..=16, "framing"),
    ];

    // When both are applied through the runner, tidy included
    let applied = applying_a_plan_of(&workspace, &seams).await;

    // Then the seam that never calls the method holds no copy of the import, and the tree compiles
    assert!(applied.is_ok(), "the apply failed: {applied:?}");
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        !the_module_named(&lib, "plainly").contains("Encode as _"),
        "the unused copy survived the tidy:\n{lib}"
    );
    assert!(
        the_module_named(&lib, "framing").contains("use crate::wire::Encode as _;"),
        "the needed import was removed:\n{lib}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_range_propagating_with_a_question_mark_returns_the_callers_imported_result_alias_and_compiles(
) {
    // Given a file importing a one-argument `Result` alias, and a range that only propagates
    let workspace = one_crate_holding(&[
        "pub mod aliased {",
        "    pub type Result<T> = std::result::Result<T, String>;",
        "}",
        "",
        "use crate::aliased::Result;",
        "",
        "fn parse(s: &str) -> Result<u32> {",
        "    s.parse::<u32>().map_err(|e| e.to_string())",
        "}",
        "",
        "pub fn plain_q(a: &str, b: &str) -> Result<u32> {",
        "    let x = parse(a)?;",
        "    let y = parse(b)?;",
        "    let z = x + y;",
        "    Ok(z * 2 + y)",
        "}",
    ]);

    // When it is extracted
    performing(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 13..=14, "parsed_pair"),
    )
    .await;

    // Then the new function returns the caller's alias, and the tree compiles
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("fn parsed_pair(b: &str, x: u32) -> Result<(u32, u32)> {"),
        "the return type is not spelled through the alias:\n{lib}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_range_in_a_module_that_defines_its_result_alias_is_spelled_with_the_alias() {
    // Given a file defining a one-argument `Result` alias, and a range that only propagates
    let workspace = one_crate_holding(&[
        "pub type Result<T> = std::result::Result<T, String>;",
        "",
        "fn parse(s: &str) -> Result<u32> {",
        "    s.parse::<u32>().map_err(|e| e.to_string())",
        "}",
        "",
        "pub fn plain_q(a: &str, b: &str) -> Result<u32> {",
        "    let x = parse(a)?;",
        "    let y = parse(b)?;",
        "    Ok(x * 2 + y)",
        "}",
    ]);

    // When the second parse is extracted
    performing(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 9..=9, "parsed"),
    )
    .await;

    // Then it returns the alias, not the qualified two-argument `Result`
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("fn parsed(b: &str) -> Result<u32> {"),
        "the return type is not spelled through the alias:\n{lib}"
    );
    assert_compiles(&workspace);
}

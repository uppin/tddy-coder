//! `read_fields_through` rebinds the `self` a range of a method reads, against a live rust-analyzer.
//!
//! A method of a type that cannot leave its crate (`E0116` anywhere else) can only have its body
//! moved once the body stops naming `self`. In the port-move pilot every `self.<field>` became
//! `state.<field>` by hand, token for token. Field mode does that edit and checks each field against
//! the state value's type on the server's answers; self mode rebinds every `self` (and writes `Self`
//! as the impl's self type), so a following `extract_method` writes a free function.
//!
//! `cargo check` (and, for the dropped borrow, `cargo clippy -D warnings`) is the assertion no edit
//! that merely looks right can satisfy.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use std::ops::RangeInclusive;

use harness::{
    a_range_over, an_extract_method_of, applying_a_plan_of, applying_keeping_the_account,
    assert_compiles_with_its_tests, assert_lints_clean, checking_deep_keeping_the_account,
    the_function_named, AFixtureWorkspace,
};
use same_crate::{an_app_holding, an_app_over_a_kernel};
use tddy_code_restructuring::RefactorOp;

/// The state value and its field types: a reference to the host's roster list and config, and a
/// copy of its interval.
const A_HOST_STATE: &str = concat!(
    "pub struct Config {\n",
    "    pub limit: u32,\n",
    "}\n",
    "\n",
    "pub struct HostState<'a> {\n",
    "    pub rosters: &'a Vec<u32>,\n",
    "    pub config: &'a Config,\n",
    "    pub interval: u64,\n",
    "}\n",
);

/// The host: it lends its fields as a `HostState` and its `total` reads three of them after its
/// first statement, beside a comment and a string that mention `self`.
const A_HOST_BODY: &str = concat!(
    "pub struct Host {\n",
    "    rosters: Vec<u32>,\n",
    "    config: Config,\n",
    "    interval: u64,\n",
    "}\n",
    "\n",
    "impl Host {\n",
    "    pub fn state(&self) -> HostState<'_> {\n",
    "        HostState {\n",
    "            rosters: &self.rosters,\n",
    "            config: &self.config,\n",
    "            interval: self.interval,\n",
    "        }\n",
    "    }\n",
    "\n",
    "    pub fn total(&self, floor: u32) -> u32 {\n",
    "        let base = floor + 1;\n",
    "        // self.rosters is every roster the host knows\n",
    "        let above = self.rosters.iter().filter(|n| **n > base).count() as u32;\n",
    "        let label = \"self.config\";\n",
    "        above + limit_of(&self.config) + lifetime_of(&self.interval) + label.len() as u32\n",
    "    }\n",
    "}\n",
    "\n",
    "fn limit_of(config: &Config) -> u32 {\n",
    "    config.limit\n",
    "}\n",
    "\n",
    "fn lifetime_of(interval: &u64) -> u32 {\n",
    "    *interval as u32\n",
    "}\n",
);

const THE_COMMENT: &str = "        // self.rosters is every roster the host knows\n";
const THE_STRING: &str = "        let label = \"self.config\";\n";

/// A crate whose `host` module holds both the state value and the host.
fn a_crate_whose_host_lends_a_state() -> AFixtureWorkspace {
    an_app_holding(&[
        ("src/lib.rs", "pub mod host;\n"),
        ("src/host.rs", &format!("{A_HOST_STATE}\n{A_HOST_BODY}")),
    ])
}

/// The one-based lines of `text` from the first holding `first` to the first after it holding
/// `last`.
fn the_lines_from(text: &str, first: &str, last: &str) -> RangeInclusive<u32> {
    let lines: Vec<&str> = text.split('\n').collect();
    let start = lines
        .iter()
        .position(|line| line.contains(first))
        .unwrap_or_else(|| panic!("no line holds `{first}`"));
    let end = start
        + lines[start..]
            .iter()
            .position(|line| line.contains(last))
            .unwrap_or_else(|| panic!("no line after `{first}` holds `{last}`"));
    (start as u32 + 1)..=(end as u32 + 1)
}

/// A `read_fields_through` over `lines` of `file`, binding `name` to `expr`.
fn a_rebind_of(
    workspace: &AFixtureWorkspace,
    file: &str,
    lines: RangeInclusive<u32>,
    name: &str,
    expr: &str,
) -> RefactorOp {
    serde_json::from_value(serde_json::json!({
        "op": "read_fields_through",
        "anchor": a_range_over(workspace, file, lines),
        "name": name,
        "expr": expr,
    }))
    .expect("a `read_fields_through` operation deserializes")
}

/// The field-mode rebind of `total`'s tail through `state`.
fn the_rebind_of_the_tail(workspace: &AFixtureWorkspace, file: &str) -> RefactorOp {
    let lines = the_lines_from(&workspace.read(file), "// self.rosters", "above + limit_of");
    a_rebind_of(workspace, file, lines, "state", "self.state()")
}

#[tokio::test(flavor = "multi_thread")]
async fn rebinds_every_field_read_of_the_range_through_the_state_value_and_the_tree_compiles() {
    // Given a host whose `total` reads three of its fields after its first statement
    let workspace = a_crate_whose_host_lends_a_state();

    // When the tail is read through `state`
    applying_a_plan_of(
        &workspace,
        &[the_rebind_of_the_tail(&workspace, "src/host.rs")],
    )
    .await
    .expect("the rebind applies");

    // Then the binding precedes the range, the range reads its fields through it, and it compiles
    let host = workspace.read("src/host.rs");
    assert!(
        host.contains(&format!(
            "        let base = floor + 1;\n        let state = self.state();\n{THE_COMMENT}"
        )),
        "the binding was not inserted before the range:\n{host}"
    );
    assert!(
        host.contains(
            "        let above = state.rosters.iter().filter(|n| **n > base).count() as u32;\n"
        ),
        "the field read was not rebound:\n{host}"
    );
    assert_compiles_with_its_tests(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_state_value_declared_in_another_crate_is_read_through_and_the_tree_compiles() {
    // Given the state value in `kernel` and the host that lends it in `app`, the port-move shape
    let workspace = an_app_over_a_kernel(
        &[
            ("src/lib.rs", "pub mod host;\n"),
            (
                "src/host.rs",
                &format!("use kernel::{{Config, HostState}};\n\n{A_HOST_BODY}"),
            ),
        ],
        &[("src/lib.rs", A_HOST_STATE)],
    );

    // When the tail is read through `state`
    applying_a_plan_of(
        &workspace,
        &[the_rebind_of_the_tail(&workspace, "app/src/host.rs")],
    )
    .await
    .expect("the rebind applies across the crate boundary");

    // Then the range reads through the other crate's type, and the workspace compiles
    let host = workspace.read("app/src/host.rs");
    assert!(
        host.contains("let state = self.state();") && host.contains("state.rosters.iter()"),
        "the range was not rebound:\n{host}"
    );
    assert_compiles_with_its_tests(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_borrow_of_a_field_the_state_holds_by_reference_loses_its_ampersand_and_a_copy_field_keeps_it(
) {
    // Given a tail that borrows `config` (a reference in the state) and `interval` (a copy in both)
    let workspace = a_crate_whose_host_lends_a_state();

    // When the tail is read through `state`
    applying_a_plan_of(
        &workspace,
        &[the_rebind_of_the_tail(&workspace, "src/host.rs")],
    )
    .await
    .expect("the rebind applies");

    // Then the reference loses its `&`, the copy keeps it, and clippy finds no needless borrow
    let host = workspace.read("src/host.rs");
    assert!(
        host.contains(
            "above + limit_of(state.config) + lifetime_of(&state.interval) + label.len() as u32"
        ),
        "the borrows were not rewritten by the state's field types:\n{host}"
    );
    assert_lints_clean(&workspace);
}

/// A host whose state value lacks one field its tail reads, and one whose state holds a field under
/// a different type: `name` is a `String` on the host and a `&str` in the state.
const A_HOST_WITH_A_MISMATCHED_STATE: &str = concat!(
    "pub struct HostState<'a> {\n",
    "    pub n: &'a u32,\n",
    "    pub name: &'a str,\n",
    "}\n",
    "\n",
    "pub struct Host {\n",
    "    n: u32,\n",
    "    extra: u32,\n",
    "    name: String,\n",
    "}\n",
    "\n",
    "impl Host {\n",
    "    pub fn state(&self) -> HostState<'_> {\n",
    "        HostState {\n",
    "            n: &self.n,\n",
    "            name: &self.name,\n",
    "        }\n",
    "    }\n",
    "\n",
    "    pub fn with_extra(&self) -> u32 {\n",
    "        let base = 1;\n",
    "        base + self.n + self.extra\n",
    "    }\n",
    "\n",
    "    pub fn name_len(&self) -> usize {\n",
    "        let base = 1;\n",
    "        base + self.name.len()\n",
    "    }\n",
    "}\n",
);

fn a_crate_whose_state_does_not_match_its_host() -> AFixtureWorkspace {
    an_app_holding(&[
        ("src/lib.rs", "pub mod host;\n"),
        ("src/host.rs", A_HOST_WITH_A_MISMATCHED_STATE),
    ])
}

/// A field-mode rebind of the one line of `src/host.rs` holding `needle`.
fn a_rebind_of_the_line_holding(workspace: &AFixtureWorkspace, needle: &str) -> RefactorOp {
    let lines = the_lines_from(&workspace.read("src/host.rs"), needle, needle);
    a_rebind_of(workspace, "src/host.rs", lines, "state", "self.state()")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_field_the_state_value_lacks_is_refused_naming_every_one_and_nothing_is_written() {
    // Given a tail that reads `extra`, which the state value does not have
    let workspace = a_crate_whose_state_does_not_match_its_host();

    // When it is read through `state`
    let refusal = applying_a_plan_of(
        &workspace,
        &[a_rebind_of_the_line_holding(
            &workspace,
            "base + self.n + self.extra",
        )],
    )
    .await
    .expect_err("a field the state lacks is refused");

    // Then the refusal names the missing field, and the file is untouched
    assert!(
        refusal.contains("`self.state()` has no field extra"),
        "the refusal does not name the missing field: {refusal}"
    );
    assert_eq!(
        workspace.read("src/host.rs"),
        A_HOST_WITH_A_MISMATCHED_STATE
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_field_whose_type_differs_by_more_than_one_reference_is_refused_naming_both_types() {
    // Given a tail that reads `name`: a `String` on the host and a `&str` in the state
    let workspace = a_crate_whose_state_does_not_match_its_host();

    // When it is read through `state`
    let refusal = applying_a_plan_of(
        &workspace,
        &[a_rebind_of_the_line_holding(
            &workspace,
            "base + self.name.len()",
        )],
    )
    .await
    .expect_err("a field of another type is refused");

    // Then the refusal names both types, and the file is untouched
    assert!(
        refusal.contains("`self.name` is `String` and `state.name` is `&str`")
            && refusal.contains("they differ by more than one reference"),
        "the refusal does not name both types: {refusal}"
    );
    assert_eq!(
        workspace.read("src/host.rs"),
        A_HOST_WITH_A_MISMATCHED_STATE
    );
}

/// A host with an associated function (no receiver) that names `Self`, and a method whose tail
/// calls a method, reads a field and names `Self` twice.
const A_HOST_THAT_NAMES_ITSELF: &str = concat!(
    "pub struct Host {\n",
    "    n: u32,\n",
    "}\n",
    "\n",
    "impl Host {\n",
    "    const BASE: u32 = 2;\n",
    "\n",
    "    pub fn build(n: u32) -> u32 {\n",
    "        let doubled = n * 2;\n",
    "        doubled + Self::BASE\n",
    "    }\n",
    "\n",
    "    fn doubled(&self) -> u32 {\n",
    "        self.n * 2\n",
    "    }\n",
    "\n",
    "    pub fn total(&self, floor: u32) -> u32 {\n",
    "        let start = floor + Self::BASE;\n",
    "        let sum = self.doubled() + self.n + start;\n",
    "        Self::clamp(sum)\n",
    "    }\n",
    "\n",
    "    fn clamp(n: u32) -> u32 {\n",
    "        n.min(100)\n",
    "    }\n",
    "\n",
    "    pub fn twice(&self) -> u32 {\n",
    "        let m = self.n;\n",
    "        m * 2\n",
    "    }\n",
    "}\n",
);

fn a_crate_whose_host_names_itself() -> AFixtureWorkspace {
    an_app_holding(&[
        ("src/lib.rs", "pub mod host;\n"),
        ("src/host.rs", A_HOST_THAT_NAMES_ITSELF),
    ])
}

#[tokio::test(flavor = "multi_thread")]
async fn a_range_in_an_associated_function_without_a_receiver_is_refused() {
    // Given an associated function, with no `self` receiver, whose tail names `Self`
    let workspace = a_crate_whose_host_names_itself();
    let lines = the_lines_from(
        A_HOST_THAT_NAMES_ITSELF,
        "doubled + Self::BASE",
        "doubled + Self::BASE",
    );

    // When its tail is rebound in self mode
    let refusal = applying_a_plan_of(
        &workspace,
        &[a_rebind_of(
            &workspace,
            "src/host.rs",
            lines,
            "backend",
            "self",
        )],
    )
    .await
    .expect_err("a range outside a method is refused");

    // Then the refusal says there is no receiver, and the file is untouched
    assert!(
        refusal.contains("the range is in no method with a `self` receiver"),
        "unexpected refusal: {refusal}"
    );
    assert_eq!(workspace.read("src/host.rs"), A_HOST_THAT_NAMES_ITSELF);
}

#[tokio::test(flavor = "multi_thread")]
async fn self_mode_rebinds_method_calls_fields_and_the_self_type_and_a_following_extract_method_writes_a_free_function(
) {
    // Given `total`, whose tail calls a method, reads a field, and names `Self` twice
    let workspace = a_crate_whose_host_names_itself();
    let lines = the_lines_from(
        A_HOST_THAT_NAMES_ITSELF,
        "let start = floor + Self::BASE;",
        "Self::clamp(sum)",
    );

    // When the tail is rebound in self mode and extracted, in one group
    let mut rebind = a_rebind_of(&workspace, "src/host.rs", lines.clone(), "backend", "self");
    rebind.group = Some("detach".to_string());
    let mut extract = an_extract_method_of(&workspace, "src/host.rs", lines, "total_of");
    extract.group = Some("detach".to_string());
    applying_a_plan_of(&workspace, &[rebind, extract])
        .await
        .expect("the rebind and the extraction apply");

    // Then the tail became a free function over the host, naming the type and not `self`, and it compiles
    let host = workspace.read("src/host.rs");
    let extracted = the_function_named(&host, "total_of");
    assert!(
        host.contains("        let backend = self;\n"),
        "the binding was not inserted:\n{host}"
    );
    assert!(
        host.contains("\nfn total_of("),
        "the extraction is not a free function:\n{host}"
    );
    assert!(
        extracted.contains("backend: &Host")
            && extracted.contains("floor + Host::BASE")
            && extracted.contains("backend.doubled() + backend.n + start")
            && extracted.contains("Host::clamp(sum)")
            && !extracted.contains("self")
            && !extracted.contains("Self"),
        "the extracted function still names the receiver or the type keyword:\n{extracted}"
    );
    assert_compiles_with_its_tests(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn self_mode_on_a_shared_receiver_binds_a_shared_reference_and_compiles() {
    // Given `twice`, a `&self` method that reads a field
    let workspace = a_crate_whose_host_names_itself();
    let lines = the_lines_from(A_HOST_THAT_NAMES_ITSELF, "let m = self.n;", "m * 2");

    // When its body is rebound in self mode
    applying_a_plan_of(
        &workspace,
        &[a_rebind_of(
            &workspace,
            "src/host.rs",
            lines,
            "backend",
            "self",
        )],
    )
    .await
    .expect("the rebind applies");

    // Then the body reads through the binding, and it compiles
    let host = workspace.read("src/host.rs");
    assert!(
        host.contains("        let backend = self;\n        let m = backend.n;\n"),
        "the body was not rebound:\n{host}"
    );
    assert_compiles_with_its_tests(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn comments_and_strings_in_the_range_that_mention_self_are_byte_identical() {
    // Given a tail holding a comment and a string that both mention `self.`
    let workspace = a_crate_whose_host_lends_a_state();

    // When the tail is read through `state`
    applying_a_plan_of(
        &workspace,
        &[the_rebind_of_the_tail(&workspace, "src/host.rs")],
    )
    .await
    .expect("the rebind applies");

    // Then the comment and the string are exactly as they were
    let host = workspace.read("src/host.rs");
    assert!(
        host.contains(THE_COMMENT) && host.contains(THE_STRING),
        "a comment or a string was rewritten:\n{host}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deep_check_reports_the_refusal_an_apply_gives_and_notes_what_an_apply_rebinds() {
    // Given a field-mode rebind over a method call, and a clean one over three field reads
    let refused_here = a_crate_whose_host_names_itself();
    let lines = the_lines_from(
        A_HOST_THAT_NAMES_ITSELF,
        "let sum = self.doubled()",
        "let sum = self.doubled()",
    );
    let over_a_method_call =
        a_rebind_of(&refused_here, "src/host.rs", lines, "state", "self.state()");
    let clean_here = a_crate_whose_host_lends_a_state();
    let clean = the_rebind_of_the_tail(&clean_here, "src/host.rs");

    // When each is checked deep and applied
    let (refused_findings, _) =
        checking_deep_keeping_the_account(&refused_here, std::slice::from_ref(&over_a_method_call))
            .await;
    let refused_apply = applying_a_plan_of(&refused_here, &[over_a_method_call]).await;
    let (clean_findings, deep_account) =
        checking_deep_keeping_the_account(&clean_here, std::slice::from_ref(&clean)).await;
    let (clean_apply, apply_account) =
        applying_keeping_the_account(&clean_here, &[clean], false).await;

    // Then both refuse the method call alike, and both note the same rebinding of the clean one
    let stem = "the range calls `self.doubled(…)` at line 19";
    let refused_findings = refused_findings.expect("the deep check runs");
    assert!(
        refused_findings
            .iter()
            .any(|finding| finding.contains(stem)),
        "the deep check did not refuse the method call: {refused_findings:?}"
    );
    assert!(
        refused_apply.expect_err("the apply refuses").contains(stem),
        "the apply did not refuse the method call alike"
    );
    assert_eq!(
        clean_findings.expect("the deep check runs"),
        Vec::<String>::new()
    );
    clean_apply.expect("the clean rebind applies");
    let note = "read_fields_through: rebound 3 reads of self through `state`";
    assert!(
        deep_account.iter().any(|line| line.contains(note))
            && apply_account.iter().any(|line| line.contains(note)),
        "the note is not in both accounts:\ndeep: {deep_account:?}\napply: {apply_account:?}"
    );
}

//! `detach_method` turns members of an inherent `impl` into free functions over a named parameter,
//! against a live rust-analyzer.
//!
//! A type's inherent impls must sit in its defining crate (E0116), so a method cannot leave with
//! the code that calls it. `detach_method` writes each member as `fn m(session: &mut T, …)` in the
//! same module with the same visibility, rewrites `self` and `Self`, and re-points every caller the
//! server knows: `x.m(a)` → `m(x, a)` (or `module::m(x, a)` from another module), `Self::m(a)` →
//! `m(a)`. Nothing is left on the type, and a block the run empties is removed.
//!
//! `cargo check` with the tests and `cargo clippy -D warnings` are the assertions no edit that merely
//! looks right can satisfy.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use harness::{
    applying_a_plan_of, applying_keeping_the_account, assert_compiles_with_its_tests,
    assert_lints_clean, checking_deep_keeping_the_account, AFixtureWorkspace,
};
use same_crate::{an_app_holding, the_anchor_over};
use tddy_code_restructuring::{Anchor, RefactorOp};

/// A `detach_method` of the members `anchor` names, their receiver becoming `name`.
fn a_detach_of(anchor: &Anchor, name: &str) -> RefactorOp {
    serde_json::from_value(serde_json::json!({
        "op": "detach_method",
        "anchor": anchor,
        "name": name,
    }))
    .expect("a `detach_method` operation deserializes")
}

/// The plan detaching `members` of `src/host.rs` (each written as the `host` module's path to it,
/// `Host::bump`) over `session`.
async fn detaching(workspace: &AFixtureWorkspace, members: &[&str]) -> RefactorOp {
    let paths: Vec<String> = members
        .iter()
        .map(|member| format!("app::host::{member}"))
        .collect();
    let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
    let anchor = the_anchor_over(workspace, "src/host.rs", &paths).await;
    a_detach_of(&anchor, "session")
}

/// Apply the detach of `members`, failing the test when it is refused.
async fn applying_the_detach_of(workspace: &AFixtureWorkspace, members: &[&str]) {
    let op = detaching(workspace, members).await;
    applying_a_plan_of(workspace, &[op])
        .await
        .expect("the detach applies");
}

/// A crate whose `host` module holds `host` and whose `tick` module holds `tick`.
fn a_crate_of(host: &str, tick: &str) -> AFixtureWorkspace {
    an_app_holding(&[
        ("src/lib.rs", "pub mod host;\npub mod tick;\n"),
        ("src/host.rs", host),
        ("src/tick.rs", tick),
    ])
}

const A_HOST_THAT_BUMPS: &str = concat!(
    "#[derive(Default)]\n",
    "pub struct Host {\n",
    "    pub(crate) n: u32,\n",
    "}\n",
    "\n",
    "impl Host {\n",
    "    pub(super) fn bump(&mut self, by: u32) -> u32 {\n",
    "        self.n += by;\n",
    "        self.n\n",
    "    }\n",
    "\n",
    "    pub(crate) fn twice(&mut self) -> u32 {\n",
    "        self.bump(1);\n",
    "        let add = |h: &mut Host| h.bump(1);\n",
    "        add(self)\n",
    "    }\n",
    "}\n",
);

const A_TICK_THAT_BUMPS: &str = concat!(
    "use crate::host::Host;\n",
    "\n",
    "pub fn tick(h: &mut Host) -> u32 {\n",
    "    // h.bump(2) is how a tick advances\n",
    "    h.bump(2) + h.twice()\n",
    "}\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn a_detached_method_becomes_a_function_over_the_session_and_every_caller_calls_it() {
    // Given `Host::bump`, called from another method, from a closure and from a sibling module
    let workspace = a_crate_of(A_HOST_THAT_BUMPS, A_TICK_THAT_BUMPS);

    // When `bump` is detached over `session`
    applying_the_detach_of(&workspace, &["Host::bump"]).await;

    // Then it is a function after the block, every caller calls it, and the tree compiles and lints clean
    let host = workspace.read("src/host.rs");
    let tick = workspace.read("src/tick.rs");
    assert!(
        host.contains("pub(super) fn bump(session: &mut Host, by: u32) -> u32 {\n    session.n += by;\n    session.n\n}\n"),
        "`bump` is not a function over `session`:\n{host}"
    );
    assert!(
        host.contains("        bump(self, 1);\n")
            && host.contains("        let add = |h: &mut Host| bump(h, 1);\n"),
        "the callers in `host` do not call the function:\n{host}"
    );
    assert!(
        !host.contains("fn bump(&mut self"),
        "the method was left on the type:\n{host}"
    );
    assert!(
        tick.contains("use crate::host;\n") && tick.contains("    host::bump(h, 2) + h.twice()\n"),
        "the caller in `tick` does not call the function through its module:\n{tick}"
    );
    assert_compiles_with_its_tests(&workspace);
    assert_lints_clean(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shared_receiver_becomes_a_shared_reference_and_an_associated_function_keeps_its_parameters(
) {
    // Given a `&self` member calling an associated function through `Self`
    let workspace = a_crate_of(
        concat!(
            "pub struct Host {\n",
            "    pub(crate) n: u32,\n",
            "}\n",
            "\n",
            "impl Host {\n",
            "    pub(crate) fn total(&self) -> u32 {\n",
            "        Self::pure(self.n)\n",
            "    }\n",
            "\n",
            "    fn pure(x: u32) -> u32 {\n",
            "        x + 1\n",
            "    }\n",
            "}\n",
        ),
        "use crate::host::Host;\n\npub fn tick(h: &Host) -> u32 {\n    h.total()\n}\n",
    );

    // When both are detached over `session`
    applying_the_detach_of(&workspace, &["Host::total", "Host::pure"]).await;

    // Then `total` takes `&Host`, `pure` keeps its parameters, and `Self::pure` is a plain call
    let host = workspace.read("src/host.rs");
    assert!(
        host.contains("pub(crate) fn total(session: &Host) -> u32 {\n    pure(session.n)\n}\n")
            && host.contains("fn pure(x: u32) -> u32 {\n    x + 1\n}\n"),
        "the members were not detached as written:\n{host}"
    );
    assert!(
        workspace
            .read("src/tick.rs")
            .contains("    host::total(h)\n"),
        "the caller in `tick` does not call the function"
    );
    assert_compiles_with_its_tests(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn self_the_type_is_written_as_the_type_and_an_owned_receiver_is_borrowed_at_the_call() {
    // Given a member naming `Self::LIMIT`, called on an owned `Host`
    let workspace = a_crate_of(
        concat!(
            "#[derive(Default)]\n",
            "pub struct Host {\n",
            "    pub(crate) n: u32,\n",
            "}\n",
            "\n",
            "impl Host {\n",
            "    pub(crate) const LIMIT: u32 = 3;\n",
            "\n",
            "    pub(crate) fn capped(&mut self) -> u32 {\n",
            "        self.n.min(Self::LIMIT)\n",
            "    }\n",
            "}\n",
        ),
        "use crate::host::Host;\n\npub fn fresh() -> u32 {\n    let mut h = Host::default();\n    h.capped()\n}\n",
    );

    // When `capped` is detached over `session`
    applying_the_detach_of(&workspace, &["Host::capped"]).await;

    // Then `Self` is the type, and the owned receiver is lent mutably at the call
    assert!(
        workspace
            .read("src/host.rs")
            .contains("pub(crate) fn capped(session: &mut Host) -> u32 {\n    session.n.min(Host::LIMIT)\n}\n"),
        "`Self` was not written as the type"
    );
    assert!(
        workspace
            .read("src/tick.rs")
            .contains("    host::capped(&mut h)\n"),
        "the owned receiver was not borrowed at the call"
    );
    assert_compiles_with_its_tests(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn doc_comments_attributes_and_visibility_travel_and_the_emptied_block_is_removed() {
    // Given a block whose one documented, attributed `pub(super)` member the crate root calls
    let workspace = an_app_holding(&[
        (
            "src/lib.rs",
            "pub mod host;\n\npub fn run(h: &mut host::Host) {\n    h.bump();\n}\n",
        ),
        (
            "src/host.rs",
            concat!(
                "pub struct Host {\n",
                "    pub(crate) n: u32,\n",
                "}\n",
                "\n",
                "impl Host {\n",
                "    /// Bump the count.\n",
                "    #[inline]\n",
                "    pub(super) fn bump(&mut self) {\n",
                "        self.n += 1;\n",
                "    }\n",
                "}\n",
            ),
        ),
    ]);

    // When the member is detached over `session`
    applying_the_detach_of(&workspace, &["Host::bump"]).await;

    // Then the function keeps its doc comment, attribute and visibility, the block is gone, and the
    // root reaches it with no widening
    let host = workspace.read("src/host.rs");
    assert!(
        host.contains("/// Bump the count.\n#[inline]\npub(super) fn bump(session: &mut Host) {\n"),
        "the trivia or the visibility did not travel:\n{host}"
    );
    assert!(
        !host.contains("impl Host {"),
        "the emptied block was left:\n{host}"
    );
    assert!(
        workspace
            .read("src/lib.rs")
            .contains("    host::bump(h);\n"),
        "the root does not call the function"
    );
    assert_compiles_with_its_tests(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn members_detached_together_call_each_other_as_functions() {
    // Given two adjacent members, the second calling the first
    let workspace = a_crate_of(
        concat!(
            "pub struct Host {\n",
            "    pub(crate) n: u32,\n",
            "}\n",
            "\n",
            "impl Host {\n",
            "    fn base(&self) -> u32 {\n",
            "        self.n\n",
            "    }\n",
            "\n",
            "    pub(crate) fn doubled(&self) -> u32 {\n",
            "        self.base() * 2\n",
            "    }\n",
            "}\n",
        ),
        "use crate::host::Host;\n\npub fn tick(h: &Host) -> u32 {\n    h.doubled()\n}\n",
    );

    // When both are detached together
    applying_the_detach_of(&workspace, &["Host::base", "Host::doubled"]).await;

    // Then the second calls the first as a function over the same session
    assert!(
        workspace
            .read("src/host.rs")
            .contains("pub(crate) fn doubled(session: &Host) -> u32 {\n    base(session) * 2\n}\n"),
        "the members do not call each other as functions"
    );
    assert_compiles_with_its_tests(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_comment_naming_the_method_is_byte_identical_and_counted_in_the_note() {
    // Given a sibling module whose comment mentions `h.bump(2)`
    let workspace = a_crate_of(A_HOST_THAT_BUMPS, A_TICK_THAT_BUMPS);
    let op = detaching(&workspace, &["Host::bump"]).await;

    // When `bump` is detached, keeping the account
    let (applied, account) = applying_keeping_the_account(&workspace, &[op], false).await;

    // Then the comment is untouched, and the note counts it
    applied.expect("the detach applies");
    assert!(
        workspace
            .read("src/tick.rs")
            .contains("    // h.bump(2) is how a tick advances\n"),
        "the comment was edited"
    );
    assert!(
        account.iter().any(|line| line.contains("detach_method:")
            && line.contains("became a function over `session`")
            && line.contains("1 comment left naming the method")),
        "the note does not count the comment: {account:?}"
    );
}

const A_HOST_WITH_MEMBERS_THAT_CANNOT_LEAVE: &str = concat!(
    "#[derive(Default)]\n",
    "pub struct Host {\n",
    "    pub(crate) n: u32,\n",
    "}\n",
    "\n",
    "pub struct Wrap<T> {\n",
    "    pub(crate) inner: T,\n",
    "}\n",
    "\n",
    "pub trait Tally {\n",
    "    fn count(&self) -> u32;\n",
    "}\n",
    "\n",
    "impl Host {\n",
    "    pub(crate) const LIMIT: u32 = 3;\n",
    "\n",
    "    pub(crate) fn with(mut self, n: u32) -> Self {\n",
    "        self.n = n;\n",
    "        self\n",
    "    }\n",
    "\n",
    "    pub fn shown(&self) -> u32 {\n",
    "        self.n\n",
    "    }\n",
    "}\n",
    "\n",
    "impl<T> Wrap<T> {\n",
    "    pub(crate) fn get(&self) -> &T {\n",
    "        &self.inner\n",
    "    }\n",
    "}\n",
    "\n",
    "impl Tally for Host {\n",
    "    fn count(&self) -> u32 {\n",
    "        self.n\n",
    "    }\n",
    "}\n",
);

/// What an apply of the detach of `member` of `src/host.rs` refuses with.
async fn the_refusal_of_detaching(workspace: &AFixtureWorkspace, member: &str) -> String {
    let op = detaching(workspace, &[member]).await;
    applying_a_plan_of(workspace, &[op])
        .await
        .expect_err("the detach is refused")
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_by_value_receiver_a_generic_impl_a_trait_member_a_pub_member_and_an_associated_const_naming_each(
) {
    // Given a host with a builder, a `pub` member, an associated const, a generic block and a trait impl
    let workspace = a_crate_of(
        A_HOST_WITH_MEMBERS_THAT_CANNOT_LEAVE,
        "use crate::host::Host;\n\npub fn tick() -> u32 {\n    Host::default().with(2).shown()\n}\n",
    );

    // When each is detached on its own
    let refusals = [
        the_refusal_of_detaching(&workspace, "Host::with").await,
        the_refusal_of_detaching(&workspace, "Wrap::get").await,
        the_refusal_of_detaching(&workspace, "<Host as Tally>::count").await,
        the_refusal_of_detaching(&workspace, "Host::shown").await,
        the_refusal_of_detaching(&workspace, "Host::LIMIT").await,
    ];

    // Then each is refused saying why, and the tree is untouched
    let reasons = [
        "`Host::with` takes `self` by value: a function over a reference cannot take it",
        "carries generic parameters or a `where` clause",
        "is a member of `impl Tally for Host`: trait members stay methods",
        "`Host::shown` is `pub` and may have callers outside the workspace",
        "`Host::LIMIT` is an associated const or type: there is nothing to detach",
    ];
    let unexplained: Vec<(&String, &&str)> = refusals
        .iter()
        .zip(&reasons)
        .filter(|(refusal, reason)| !refusal.contains(**reason))
        .collect();
    assert!(unexplained.is_empty(), "{unexplained:?}");
    assert_eq!(
        workspace.read("src/host.rs"),
        A_HOST_WITH_MEMBERS_THAT_CANNOT_LEAVE
    );
}

const A_HOST_WHOSE_CALLERS_CANNOT_BE_REWRITTEN: &str = concat!(
    "#[derive(Default)]\n",
    "pub struct Host {\n",
    "    pub(crate) n: u32,\n",
    "}\n",
    "\n",
    "impl Host {\n",
    "    pub(crate) fn shadowing(&self) -> u32 {\n",
    "        let session = self.n;\n",
    "        session + 1\n",
    "    }\n",
    "\n",
    "    pub(crate) fn taken(&self) -> u32 {\n",
    "        self.n\n",
    "    }\n",
    "\n",
    "    pub(crate) fn boxed(&self) -> u32 {\n",
    "        self.n\n",
    "    }\n",
    "\n",
    "    pub(crate) fn printed(&self) -> u32 {\n",
    "        self.n\n",
    "    }\n",
    "}\n",
    "\n",
    "pub(crate) fn taken() -> u32 {\n",
    "    0\n",
    "}\n",
);

const A_TICK_WHOSE_CALLS_CANNOT_BE_REWRITTEN: &str = concat!(
    "use crate::host::Host;\n",
    "\n",
    "pub fn tick(h: &Host) -> String {\n",
    "    let a = h.shadowing() + h.taken() + crate::host::taken();\n",
    "    let b: Box<Host> = Box::default();\n",
    "    let c = b.boxed();\n",
    "    format!(\"{a} {c} {}\", h.printed())\n",
    "}\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_name_the_body_writes_a_name_the_module_binds_a_boxed_receiver_and_a_call_inside_a_macro_naming_each(
) {
    // Given members whose body writes `session`, whose name the module already declares, whose call
    // has a boxed receiver, and whose call sits inside `format!`
    let workspace = a_crate_of(
        A_HOST_WHOSE_CALLERS_CANNOT_BE_REWRITTEN,
        A_TICK_WHOSE_CALLS_CANNOT_BE_REWRITTEN,
    );

    // When each is detached on its own
    let refusals = [
        the_refusal_of_detaching(&workspace, "Host::shadowing").await,
        the_refusal_of_detaching(&workspace, "Host::taken").await,
        the_refusal_of_detaching(&workspace, "Host::boxed").await,
        the_refusal_of_detaching(&workspace, "Host::printed").await,
    ];

    // Then each is refused naming its site, and neither file is touched
    let reasons = [
        "`session` is already written at line 8 of `shadowing`: the parameter would shadow it",
        "already declares or imports `taken`",
        "has receiver `b` of type `Box<Host>`, which is neither `Host` nor a reference to it",
        "is inside a macro invocation, which this run does not rewrite",
    ];
    let unexplained: Vec<(&String, &&str)> = refusals
        .iter()
        .zip(&reasons)
        .filter(|(refusal, reason)| !refusal.contains(**reason))
        .collect();
    assert!(unexplained.is_empty(), "{unexplained:?}");
    assert_eq!(
        (workspace.read("src/host.rs"), workspace.read("src/tick.rs")),
        (
            A_HOST_WHOSE_CALLERS_CANNOT_BE_REWRITTEN.to_string(),
            A_TICK_WHOSE_CALLS_CANNOT_BE_REWRITTEN.to_string()
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deep_check_reports_the_refusal_an_apply_gives_and_the_note_an_apply_prints() {
    // Given a builder that cannot be detached, and a member that can
    let refused_here = a_crate_of(
        A_HOST_WITH_MEMBERS_THAT_CANNOT_LEAVE,
        "use crate::host::Host;\n\npub fn tick() -> u32 {\n    Host::default().with(2).shown()\n}\n",
    );
    let the_builder = detaching(&refused_here, &["Host::with"]).await;
    let clean_here = a_crate_of(A_HOST_THAT_BUMPS, A_TICK_THAT_BUMPS);
    let clean = detaching(&clean_here, &["Host::bump"]).await;

    // When each is checked deep and applied
    let (refused_findings, _) =
        checking_deep_keeping_the_account(&refused_here, std::slice::from_ref(&the_builder)).await;
    let refused_apply = applying_a_plan_of(&refused_here, &[the_builder]).await;
    let (clean_findings, deep_account) =
        checking_deep_keeping_the_account(&clean_here, std::slice::from_ref(&clean)).await;
    let (clean_apply, apply_account) =
        applying_keeping_the_account(&clean_here, &[clean], false).await;

    // Then both refuse the builder alike, and both note the same detach of the clean member
    let stem = "`Host::with` takes `self` by value";
    let refused_findings = refused_findings.expect("the deep check runs");
    assert!(
        refused_findings
            .iter()
            .any(|finding| finding.contains(stem)),
        "the deep check did not refuse the builder: {refused_findings:?}"
    );
    assert!(
        refused_apply.expect_err("the apply refuses").contains(stem),
        "the apply did not refuse the builder alike"
    );
    assert_eq!(
        clean_findings.expect("the deep check runs"),
        Vec::<String>::new()
    );
    clean_apply.expect("the clean detach applies");
    let note = "detach_method: Host::bump became a function over `session`";
    assert!(
        deep_account.iter().any(|line| line.contains(note))
            && apply_account.iter().any(|line| line.contains(note)),
        "the note is not in both accounts:\ndeep: {deep_account:?}\napply: {apply_account:?}"
    );
}

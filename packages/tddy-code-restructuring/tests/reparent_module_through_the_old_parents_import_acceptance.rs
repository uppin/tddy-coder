//! `reparent_module` of a module that reaches a name its old parent only **imports**.
//!
//! Found re-parenting `local_exec_tool_dispatch`: it wrote `use super::DaemonSessionHost;`, and the old
//! parent had that name through a private `use` of its own. Written again for the new parent the path
//! became `super::super::svc_resolve_os_user::DaemonSessionHost`, which names a private import from
//! outside the module that holds it. The path has to follow the import to what it brings in.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use harness::assert_compiles_with_its_tests;
use same_crate::{an_app_holding, reparenting_module};

#[tokio::test(flavor = "multi_thread")]
async fn follows_the_old_parents_private_import_to_the_item_it_brings_in() {
    // Given a module that names `Config` through its parent's private `use` of it
    let workspace = an_app_holding(&[
        (
            "src/lib.rs",
            "pub mod host;\npub mod split;\npub mod types;\n",
        ),
        ("src/types.rs", "pub struct Config(pub u32);\n"),
        (
            "src/host.rs",
            concat!(
                "use crate::types::Config;\n",
                "\n",
                "mod worker;\n",
                "\n",
                "pub fn make() -> Config {\n",
                "    Config(1)\n",
                "}\n",
            ),
        ),
        (
            "src/host/worker.rs",
            "use super::Config;\n\npub fn describe(config: &Config) -> u32 {\n    config.0\n}\n",
        ),
        ("src/split.rs", "pub fn marker() {}\n"),
    ]);

    // When it moves under `split`
    reparenting_module(&workspace, "src/host.rs", "worker", "app::split", None)
        .await
        .expect("the move applies");

    // Then the path reaches `Config` where it is defined, not through the old parent's import
    let moved = workspace.read("src/split/worker.rs");
    assert!(
        !moved.contains("host::Config"),
        "the path still goes through the old parent's private import:\n{moved}"
    );
    assert_compiles_with_its_tests(&workspace);
}

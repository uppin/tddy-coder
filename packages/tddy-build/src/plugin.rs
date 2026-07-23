//! The plugin wiring point: a [`BuildPlugin`] lowers a target's open config into
//! [`BuildAction`]s, and a [`PluginRegistry`] maps `config.type` tags to plugins.
//!
//! `tddy-build` carries no recipe knowledge and depends on no plugin crate. The
//! language/ecosystem recipes (`tddy-build-rust`, …) implement this trait, and the
//! binaries assemble a registry and pass it into the engine.

use std::collections::HashMap;
use std::sync::Arc;

use crate::capabilities::BuildMode;
use crate::error::BuildError;
use crate::proto::BuildAction;

/// Everything a plugin needs to lower one target.
pub struct LowerContext<'a> {
    /// The `config.type` tag that selected this plugin (a plugin may handle several).
    pub type_name: &'a str,
    pub target_id: &'a str,
    pub target_name: &'a str,
    pub deps: &'a [String],
    /// The target's config fields (everything under `config:` except `type`).
    pub config: &'a serde_yaml::Value,
}

/// Lowers a family of target `type`s into concrete build actions.
pub trait BuildPlugin: Send + Sync {
    /// The `config.type` tags this plugin handles, e.g. `["rust_binary", "rust_library"]`.
    fn type_names(&self) -> &'static [&'static str];

    /// Lower the target described by `ctx` into its ordered build actions (the compile lifecycle).
    fn lower(&self, ctx: &LowerContext) -> Result<Vec<BuildAction>, BuildError>;

    /// Lower the target for a specific lifecycle `mode`. The default supports only
    /// [`BuildMode::Compile`] (delegating to [`Self::lower`]); a plugin overrides this to emit
    /// test/run actions. An unsupported mode is a hard error — never a silent fallback to compile.
    fn lower_mode(
        &self,
        ctx: &LowerContext,
        mode: BuildMode,
    ) -> Result<Vec<BuildAction>, BuildError> {
        match mode {
            BuildMode::Compile => self.lower(ctx),
            other => Err(BuildError::Manifest(format!(
                "target type '{}' does not support {}",
                ctx.type_name,
                other.label()
            ))),
        }
    }
}

/// Maps target `type` tags to the plugins that handle them.
#[derive(Default)]
pub struct PluginRegistry {
    by_type: HashMap<String, Arc<dyn BuildPlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `plugin` under each of its declared type names. A later registration
    /// for the same type replaces an earlier one.
    pub fn register(&mut self, plugin: Arc<dyn BuildPlugin>) -> &mut Self {
        for name in plugin.type_names() {
            self.by_type.insert((*name).to_string(), plugin.clone());
        }
        self
    }

    /// Look up the plugin handling `type_name`, or `None` if none is registered.
    pub fn get(&self, type_name: &str) -> Option<&Arc<dyn BuildPlugin>> {
        self.by_type.get(type_name)
    }

    /// All registered type names, in arbitrary order.
    pub fn registered_types(&self) -> impl Iterator<Item = &str> {
        self.by_type.keys().map(String::as_str)
    }
}

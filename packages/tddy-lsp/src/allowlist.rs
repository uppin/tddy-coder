//! Language identity, launch specs, and the allow-list that gates which languages may
//! spawn a server. The two steps are deliberately separate: [`language_for_target_type`]
//! maps a build-target type to a [`LanguageId`] (pure), and [`LspAllowList`] decides policy
//! (which languages are permitted, and how to launch them).

use serde_json::Value;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;

/// A programming language, decoupled from any specific server binary.
///
/// Open rather than an enum: a built-in language and one an installed extension brings are told
/// apart by nothing but this name. Two ids are the same language exactly when their strings are
/// equal, whether either was spelled as a constant or read from a manifest.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LanguageId(Cow<'static, str>);

impl LanguageId {
    /// The built-in Rust language.
    pub const RUST: LanguageId = LanguageId(Cow::Borrowed("rust"));

    /// The language named `id`, as a manifest or a caller spells it.
    pub fn new(id: &str) -> Self {
        Self(Cow::Owned(id.to_string()))
    }

    /// The id as a string — also the LSP `languageId` unless a [`LaunchSpec`] says otherwise.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LanguageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How to launch a language server for one language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    /// Program to execute (PATH-resolved unless absolute).
    pub program: String,
    /// Arguments passed to the program.
    pub args: Vec<String>,
    /// Extra environment variables for the server process.
    pub env: Vec<(String, String)>,
    /// What the client advertises in the `initialize` handshake.
    ///
    /// A language server tailors its answers to this, and rust-analyzer in particular returns no
    /// code actions at all to a client that advertised no `codeAction` support — which is
    /// indistinguishable from a range that supports no refactoring. Empty by default, because
    /// the queries this registry was built for (definition, references, hover, symbols) need
    /// nothing declared.
    pub capabilities: Value,
    /// The server's own settings, sent as `initializationOptions` in the same handshake.
    ///
    /// Null by default, which leaves the server on its defaults.
    pub initialization_options: Value,
    /// The LSP `languageId` a document of this language is opened as.
    ///
    /// Empty from [`LaunchSpec::new`]; [`LspAllowList::allow`] fills an empty one with the id of the
    /// language it is allowed for, which is what a server of that language expects to be told.
    pub language_id: String,
}

impl LaunchSpec {
    /// A launch spec for `program` with no args or extra env.
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            capabilities: Value::Object(Default::default()),
            initialization_options: Value::Null,
            language_id: String::new(),
        }
    }

    /// Advertise `capabilities` in the `initialize` handshake.
    pub fn with_capabilities(mut self, capabilities: Value) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Send `options` as the handshake's `initializationOptions`.
    pub fn with_initialization_options(mut self, options: Value) -> Self {
        self.initialization_options = options;
        self
    }

    /// Open documents as `language_id` rather than as the language's own id.
    pub fn with_language_id(mut self, language_id: impl Into<String>) -> Self {
        self.language_id = language_id.into();
        self
    }
}

/// The gate: only languages present here may spawn a server. Rust-only by default via
/// [`LspAllowList::rust_only`], but any language + [`LaunchSpec`] can be added.
#[derive(Debug, Clone, Default)]
pub struct LspAllowList {
    by_language: HashMap<LanguageId, LaunchSpec>,
}

impl LspAllowList {
    /// An empty allow-list — no language is permitted.
    pub fn new() -> Self {
        Self::default()
    }

    /// Permit `language`, launching it with `spec`.
    ///
    /// A spec that names no `languageId` opens its documents as `language`'s own id.
    pub fn allow(&mut self, language: LanguageId, mut spec: LaunchSpec) -> &mut Self {
        if spec.language_id.is_empty() {
            spec.language_id = language.as_str().to_string();
        }
        self.by_language.insert(language, spec);
        self
    }

    /// Whether a server may be spawned for `language`.
    pub fn is_allowed(&self, language: &LanguageId) -> bool {
        self.by_language.contains_key(language)
    }

    /// The launch spec for `language`, if permitted.
    pub fn launch_spec(&self, language: &LanguageId) -> Option<&LaunchSpec> {
        self.by_language.get(language)
    }

    /// The default production allow-list: Rust via `rust-analyzer` on PATH.
    pub fn rust_only() -> Self {
        let mut allow = Self::new();
        allow.allow(LanguageId::RUST, LaunchSpec::new("rust-analyzer"));
        allow
    }
}

/// Map a build-target `config.type` to the language whose server should serve it.
///
/// `rust_binary` / `rust_library` → [`LanguageId::RUST`]; unknown types → `None`.
pub fn language_for_target_type(type_name: &str) -> Option<LanguageId> {
    match type_name {
        "rust_binary" | "rust_library" => Some(LanguageId::RUST),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_binary_target_type_maps_to_the_rust_language() {
        // Given a Rust binary build-target type
        let target_type = "rust_binary";

        // When we resolve its language
        let language = language_for_target_type(target_type);

        // Then it is Rust
        assert_eq!(language, Some(LanguageId::RUST));
    }

    #[test]
    fn rust_library_target_type_maps_to_the_rust_language() {
        // Given a Rust library build-target type
        let target_type = "rust_library";

        // When we resolve its language
        let language = language_for_target_type(target_type);

        // Then it is Rust
        assert_eq!(language, Some(LanguageId::RUST));
    }

    #[test]
    fn an_unknown_target_type_maps_to_no_language() {
        // Given a target type with no language server
        let target_type = "docker";

        // When we resolve its language
        let language = language_for_target_type(target_type);

        // Then there is none
        assert_eq!(language, None);
    }

    #[test]
    fn the_default_allow_list_permits_rust() {
        // Given the default production allow-list
        let allow = LspAllowList::rust_only();

        // When we ask whether Rust is permitted
        let permitted = allow.is_allowed(&LanguageId::RUST);

        // Then it is
        assert!(permitted, "expected the default allow-list to permit Rust");
    }
}

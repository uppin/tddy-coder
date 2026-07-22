//! Language identity, launch specs, and the allow-list that gates which languages may
//! spawn a server. The two steps are deliberately separate: [`language_for_target_type`]
//! maps a build-target type to a [`Language`] (pure), and [`LspAllowList`] decides policy
//! (which languages are permitted, and how to launch them).

use std::collections::HashMap;

/// A programming language, decoupled from any specific server binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
}

impl Language {
    /// The LSP `languageId` string for this language.
    pub fn id(&self) -> &'static str {
        match self {
            Language::Rust => "rust",
        }
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
}

impl LaunchSpec {
    /// A launch spec for `program` with no args or extra env.
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
        }
    }
}

/// The gate: only languages present here may spawn a server. Rust-only by default via
/// [`LspAllowList::rust_only`], but any language + [`LaunchSpec`] can be added.
#[derive(Debug, Clone, Default)]
pub struct LspAllowList {
    by_language: HashMap<Language, LaunchSpec>,
}

impl LspAllowList {
    /// An empty allow-list — no language is permitted.
    pub fn new() -> Self {
        Self::default()
    }

    /// Permit `language`, launching it with `spec`.
    pub fn allow(&mut self, language: Language, spec: LaunchSpec) -> &mut Self {
        self.by_language.insert(language, spec);
        self
    }

    /// Whether a server may be spawned for `language`.
    pub fn is_allowed(&self, language: Language) -> bool {
        self.by_language.contains_key(&language)
    }

    /// The launch spec for `language`, if permitted.
    pub fn launch_spec(&self, language: Language) -> Option<&LaunchSpec> {
        self.by_language.get(&language)
    }

    /// The default production allow-list: Rust via `rust-analyzer` on PATH.
    pub fn rust_only() -> Self {
        let mut allow = Self::new();
        allow.allow(Language::Rust, LaunchSpec::new("rust-analyzer"));
        allow
    }
}

/// Map a build-target `config.type` to the language whose server should serve it.
///
/// `rust_binary` / `rust_library` → [`Language::Rust`]; unknown types → `None`.
pub fn language_for_target_type(type_name: &str) -> Option<Language> {
    match type_name {
        "rust_binary" | "rust_library" => Some(Language::Rust),
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
        assert_eq!(language, Some(Language::Rust));
    }

    #[test]
    fn rust_library_target_type_maps_to_the_rust_language() {
        // Given a Rust library build-target type
        let target_type = "rust_library";

        // When we resolve its language
        let language = language_for_target_type(target_type);

        // Then it is Rust
        assert_eq!(language, Some(Language::Rust));
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
        let permitted = allow.is_allowed(Language::Rust);

        // Then it is
        assert!(permitted, "expected the default allow-list to permit Rust");
    }
}

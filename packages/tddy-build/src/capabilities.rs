//! Build lifecycle modes + resolution of a target's BSP metadata (capabilities / tags / languages)
//! from its declared fields, falling back to derivation from `config.type`.

use crate::manifest::{BuildTarget, TargetCapabilities};

/// The build lifecycle operation being requested for a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildMode {
    Compile,
    Test,
    Run,
}

impl BuildMode {
    /// Lowercase label, used in capability-rejection messages.
    pub fn label(&self) -> &'static str {
        match self {
            BuildMode::Compile => "compile",
            BuildMode::Test => "test",
            BuildMode::Run => "run",
        }
    }
}

/// A target's resolved BSP metadata: declared values win, else derived from `config.type`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMeta {
    pub tags: Vec<String>,
    pub languages: Vec<String>,
    pub capabilities: TargetCapabilities,
}

/// Type-derived defaults for a target, before author-declared overrides are applied.
struct Derived {
    tags: Vec<String>,
    languages: Vec<String>,
    capabilities: TargetCapabilities,
}

/// Derive metadata from `config.type` (or empty defaults when there is no config).
fn derive_from_type(target: &BuildTarget) -> Derived {
    let empty = Derived {
        tags: Vec::new(),
        languages: Vec::new(),
        capabilities: TargetCapabilities::default(),
    };
    let Some(config) = target.config.as_ref() else {
        return empty;
    };
    let caps = |compile, test, run| TargetCapabilities {
        compile,
        test,
        run,
        debug: false,
    };
    match config.r#type.as_str() {
        "rust_binary" => Derived {
            tags: vec!["application".to_string()],
            languages: vec!["rust".to_string()],
            capabilities: caps(true, true, true),
        },
        "rust_library" => Derived {
            tags: vec!["library".to_string()],
            languages: vec!["rust".to_string()],
            capabilities: caps(true, true, false),
        },
        "typescript" => Derived {
            tags: Vec::new(),
            languages: vec!["typescript".to_string()],
            capabilities: caps(true, true, true),
        },
        "docker_image" | "buildroot_image" | "qemu_disk_image" => Derived {
            tags: vec!["application".to_string()],
            languages: Vec::new(),
            capabilities: caps(true, false, false),
        },
        "script" => Derived {
            tags: Vec::new(),
            languages: Vec::new(),
            capabilities: caps(true, false, false),
        },
        // `tool` / `group` are structural: no capabilities, tags, or languages.
        _ => empty,
    }
}

/// Resolve a target's tags/languages/capabilities. Author-declared fields take precedence; anything
/// omitted is derived from `config.type` (`rust_binary`/`rust_library`/`typescript`/`docker_image`/…).
pub fn resolve_target_metadata(target: &BuildTarget) -> ResolvedMeta {
    let derived = derive_from_type(target);
    ResolvedMeta {
        tags: if target.tags.is_empty() {
            derived.tags
        } else {
            target.tags.clone()
        },
        languages: if target.languages.is_empty() {
            derived.languages
        } else {
            target.languages.clone()
        },
        capabilities: target.capabilities.unwrap_or(derived.capabilities),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{load_build_manifest, BuildManifest};

    fn target(yaml: &str) -> BuildTarget {
        let manifest: BuildManifest = load_build_manifest(yaml).expect("manifest parses");
        manifest.targets.into_iter().next().expect("one target")
    }

    fn one(yaml_target: &str) -> BuildTarget {
        target(&format!("schema_version: 1\ntargets:\n{yaml_target}"))
    }

    #[test]
    fn derives_rust_library_as_a_compilable_testable_library() {
        // Given — a rust_library target with no declared metadata.
        let t = one(
            "  - id: \"p:lib\"\n    name: Lib\n    config: { type: rust_library, package: p }\n",
        );

        // When
        let meta = resolve_target_metadata(&t);

        // Then
        assert_eq!(meta.languages, vec!["rust".to_string()]);
        assert_eq!(meta.tags, vec!["library".to_string()]);
        assert!(meta.capabilities.compile && meta.capabilities.test);
        assert!(!meta.capabilities.run, "a library is not runnable");
    }

    #[test]
    fn derives_rust_binary_as_a_runnable_application() {
        // Given
        let t = one(
            "  - id: \"p:app\"\n    name: App\n    config: { type: rust_binary, package: p }\n",
        );

        // When
        let meta = resolve_target_metadata(&t);

        // Then
        assert_eq!(meta.tags, vec!["application".to_string()]);
        assert!(meta.capabilities.compile && meta.capabilities.test && meta.capabilities.run);
    }

    #[test]
    fn derives_docker_image_as_compile_only() {
        // Given
        let t =
            one("  - id: \"p:img\"\n    name: Img\n    config: { type: docker_image, tag: x }\n");

        // When
        let meta = resolve_target_metadata(&t);

        // Then
        assert!(meta.capabilities.compile);
        assert!(!meta.capabilities.test && !meta.capabilities.run);
    }

    #[test]
    fn declared_capabilities_and_tags_override_derivation() {
        // Given — a rust_binary (would derive run=true) that explicitly forbids run and retags.
        let t = one(
            "  - id: \"p:app\"\n    name: App\n    tags: [service]\n    languages: [rust, proto]\n    \
             capabilities: { compile: true, test: false, run: false, debug: false }\n    \
             config: { type: rust_binary, package: p }\n",
        );

        // When
        let meta = resolve_target_metadata(&t);

        // Then — declared values win over the type-based derivation.
        assert_eq!(meta.tags, vec!["service".to_string()]);
        assert_eq!(
            meta.languages,
            vec!["rust".to_string(), "proto".to_string()]
        );
        assert!(meta.capabilities.compile);
        assert!(
            !meta.capabilities.test,
            "declared test:false wins over derived test:true"
        );
        assert!(!meta.capabilities.run);
    }
}

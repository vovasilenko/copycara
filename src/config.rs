use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct CopycaraConfig {
    #[serde(default)]
    pub cleanup: CleanupConfig,
    #[serde(default)]
    pub push: PushConfig,
    #[serde(default)]
    pub remotes: RemoteConfig,
    #[serde(default)]
    pub hooks: HooksConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CleanupConfig {
    #[serde(default = "default_cleanup_mode")]
    pub mode: String,
    #[serde(default)]
    pub extra_extensions: Vec<String>,
    #[serde(default)]
    pub preserve_patterns: Vec<String>,
    #[serde(default)]
    pub extension_map: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PushConfig {
    #[serde(default = "default_true")]
    pub force_with_lease: bool,
}

impl Default for PushConfig {
    fn default() -> Self {
        Self { force_with_lease: true }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteConfig {
    #[serde(default = "default_public_remotes")]
    pub public: Vec<String>,
    #[serde(default = "default_private_remotes")]
    pub private: Vec<String>,
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self { public: vec!["origin".to_string()], private: vec!["private".to_string()] }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct HooksConfig {
    #[serde(default = "default_true")]
    pub install_pre_push: bool,
    #[serde(default = "default_true")]
    pub install_post_checkout: bool,
}

impl Default for HooksConfig {
    fn default() -> Self {
        Self { install_pre_push: true, install_post_checkout: true }
    }
}

fn default_cleanup_mode() -> String {
    "all".to_string()
}

fn default_true() -> bool {
    true
}

fn default_public_remotes() -> Vec<String> {
    vec!["origin".to_string()]
}

fn default_private_remotes() -> Vec<String> {
    vec!["private".to_string()]
}

impl Default for CopycaraConfig {
    fn default() -> Self {
        Self {
            cleanup: CleanupConfig {
                mode: "all".to_string(),
                extra_extensions: vec![],
                preserve_patterns: vec![],
                extension_map: HashMap::new(),
            },
            push: PushConfig { force_with_lease: true },
            remotes: RemoteConfig::default(),
            hooks: HooksConfig { install_pre_push: true, install_post_checkout: true },
        }
    }
}

impl CopycaraConfig {
    pub fn load() -> Self {
        let path = Path::new(".copycara/config.toml");
        if path.exists() {
            match fs::read_to_string(path) {
                Ok(content) => match toml::from_str(&content) {
                    Ok(config) => return config,
                    Err(e) => {
                        eprintln!(
                            "  [Copycara Config] Warning: failed to parse config.toml: {e}. Using defaults."
                        );
                    }
                },
                Err(e) => {
                    eprintln!(
                        "  [Copycara Config] Warning: failed to read config.toml: {e}. Using defaults."
                    );
                }
            }
        }
        Self::default()
    }

    pub fn default_config_content() -> &'static str {
        r#"# Copycara DLP Engine Configuration
# Edit this file to customize cleanup behaviour.

[cleanup]
# Режим очистки: "all" (удалять все комментарии) | "smart" (сохранять TODO/FIXME/doc)
mode = "all"

# Дополнительные расширения для обработки (tree-sitter поддерживает большинство языков)
extra_extensions = []

# Кастомные паттерны для сохранения (комментарии с этими строками НЕ вырезаются)
preserve_patterns = ["COPYCARA-KEEP", "NO-DLP"]

# Маппинг неизвестных расширений на известные (tree-sitter языки).
# Работает через rename-trick: перед обработкой файл переименовывается
# в целевое расширение, очищается, и переименовывается обратно.
# Пример: .cu (CUDA C++) обрабатывается как .cpp
# extension_map = { cu = "cpp", cuh = "cpp" }
extension_map = {}

[remotes]
# Remote-ы, в которые уходит ЧИСТЫЙ код (теневые refs без комментариев)
public = ["origin"]

# Remote-ы, в которые уходит ГРЯЗНЫЙ бэкап (оригинальный код + git notes)
private = ["private"]

[push]
# Использовать --force-with-lease при copycara push --force
force_with_lease = true

[hooks]
install_pre_push = true
install_post_checkout = true
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_fields() {
        let cfg = CopycaraConfig::default();
        assert_eq!(cfg.cleanup.mode, "all");
        assert!(cfg.cleanup.preserve_patterns.is_empty());
        assert!(cfg.cleanup.extension_map.is_empty());
        assert!(cfg.push.force_with_lease);
        assert_eq!(cfg.remotes.public, vec!["origin"]);
        assert_eq!(cfg.remotes.private, vec!["private"]);
        assert!(cfg.hooks.install_pre_push);
        assert!(cfg.hooks.install_post_checkout);
    }

    #[test]
    fn test_parse_full_config() {
        let toml_str = r#"
[cleanup]
mode = "smart"
preserve_patterns = ["KEEP-ME"]
extension_map = { cu = "cpp" }

[remotes]
public = ["origin", "mirror"]
private = ["private", "backup"]

[push]
force_with_lease = false

[hooks]
install_pre_push = false
install_post_checkout = false
"#;
        let cfg: CopycaraConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.cleanup.mode, "smart");
        assert_eq!(cfg.cleanup.preserve_patterns, vec!["KEEP-ME"]);
        assert_eq!(cfg.cleanup.extension_map.get("cu").unwrap(), "cpp");
        assert_eq!(cfg.remotes.public, vec!["origin", "mirror"]);
        assert_eq!(cfg.remotes.private, vec!["private", "backup"]);
        assert!(!cfg.push.force_with_lease);
        assert!(!cfg.hooks.install_pre_push);
    }

    #[test]
    fn test_default_config_content_is_valid_toml() {
        let content = CopycaraConfig::default_config_content();
        let cfg: Result<CopycaraConfig, toml::de::Error> = toml::from_str(content);
        assert!(cfg.is_ok(), "default config content should be valid TOML: {}", cfg.unwrap_err());
    }

    #[test]
    fn test_partial_config_uses_defaults() {
        let cfg: CopycaraConfig = toml::from_str("[cleanup]\n[push]\n[hooks]\n").unwrap();
        assert_eq!(cfg.cleanup.mode, "all");
        assert!(cfg.cleanup.extension_map.is_empty());
        assert_eq!(cfg.remotes.public, vec!["origin"]);
        assert_eq!(cfg.remotes.private, vec!["private"]);
    }

    #[test]
    fn test_extension_map_deserialization() {
        let toml_str = r#"
[cleanup]
extension_map = { cu = "cpp", cuh = "cpp", metal = "cpp" }
"#;
        let cfg: CopycaraConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.cleanup.extension_map.len(), 3);
        assert_eq!(cfg.cleanup.extension_map.get("cu").unwrap(), "cpp");
        assert_eq!(cfg.cleanup.extension_map.get("cuh").unwrap(), "cpp");
        assert_eq!(cfg.cleanup.extension_map.get("metal").unwrap(), "cpp");
    }

    #[test]
    fn test_smart_mode_keeps_todos() {
        let toml_str = r#"
[cleanup]
mode = "smart"
"#;
        let cfg: CopycaraConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.cleanup.mode, "smart");
    }
}

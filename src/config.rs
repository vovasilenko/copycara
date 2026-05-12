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

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PushConfig {
    #[serde(default = "default_true")]
    pub auto_push_private: bool,
    #[serde(default = "default_true")]
    pub force_with_lease: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct HooksConfig {
    #[serde(default = "default_true")]
    pub install_pre_push: bool,
    #[serde(default = "default_true")]
    pub install_post_checkout: bool,
}

fn default_cleanup_mode() -> String {
    "all".to_string()
}

fn default_true() -> bool {
    true
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
            push: PushConfig {
                auto_push_private: true,
                force_with_lease: true,
            },
            hooks: HooksConfig {
                install_pre_push: true,
                install_post_checkout: true,
            },
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
                            "  [Copycara Config] Warning: failed to parse config.toml: {}. Using defaults.",
                            e
                        );
                    }
                },
                Err(e) => {
                    eprintln!(
                        "  [Copycara Config] Warning: failed to read config.toml: {}. Using defaults.",
                        e
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

[push]
auto_push_private = true
force_with_lease = true

[hooks]
install_pre_push = true
install_post_checkout = true
"#
    }
}

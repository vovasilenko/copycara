//! DLP comment scrubbing via uncomment (tree-sitter AST).
//!
//! Walks a directory tree and removes comments from recognised source files.
//! Supports extension mapping for unsupported file types (rename-trick).

use crate::config::CopycaraConfig;
use crate::ignore::IgnoreRules;
use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use uncomment::config::{Config, ConfigManager};
use uncomment::processor::{ProcessingOptions, Processor};

const VALID_EXTENSIONS: &[&str] = &[
    "py", "pyw", "pyi", "pyx", "pxd", "rs", "js", "jsx", "mjs", "cjs", "ts", "tsx", "mts", "cts",
    "cpp", "cxx", "cc", "c++", "hpp", "hxx", "hh", "h++", "c", "h", "java", "scala", "sc", "go",
    "cs", "rb", "rbw", "gemspec", "rake", "sh", "bash", "zsh", "fish", "php", "phtml", "swift",
    "kt", "kts", "lua", "hs", "lhs", "jl", "ex", "exs", "erl", "hrl", "dart", "zig", "r", "R",
    "clj", "cljs", "cljc", "edn", "elm", "groovy", "gradle", "ml", "mli", "f90", "f95", "f03",
    "f08", "pl", "pm", "vue", "svelte", "css", "scss", "sql", "html", "htm", "xhtml", "xml", "xsd",
    "xsl", "xslt", "svg", "json", "jsonc", "yaml", "yml", "toml", "ini", "cfg", "conf", "hcl",
    "tf", "tfvars", "proto", "nix", "tex", "sty", "cls", "ps1", "psm1", "psd1", "mk",
];

pub fn apply_dlp_cleanup(dir: &Path) -> Result<()> {
    println!("  [Copycara Engine] Applying uncomment (tree-sitter AST)...");

    let cfg = CopycaraConfig::load();
    let (remove_todo, remove_fixme, remove_doc) = match cfg.cleanup.mode.as_str() {
        "smart" => (false, false, false),
        _ => (true, true, true),
    };
    let ext_map = cfg.cleanup.extension_map.clone();
    let extra_extensions = cfg.cleanup.extra_extensions.clone();
    let ignore = IgnoreRules::load();

    let mut processor = Processor::new();
    let config_manager = ConfigManager::from_single_config(dir, Config::default())?;
    let options = ProcessingOptions {
        remove_todo,
        remove_fixme,
        remove_doc,
        custom_preserve_patterns: cfg.cleanup.preserve_patterns.clone(),
        use_default_ignores: false,
        dry_run: false,
        show_diff: false,
        respect_gitignore: false,
        traverse_git_repos: false,
    };

    let base_dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let mut ctx = VisitContext {
        processor: &mut processor,
        config_manager: &config_manager,
        options: &options,
        ext_map: &ext_map,
        extra_extensions: &extra_extensions,
        ignore: &ignore,
        base_dir: &base_dir,
    };
    visit_dirs(&base_dir, &mut ctx)?;
    Ok(())
}

fn process_mapped(
    path: &Path,
    target_ext: &str,
    processor: &mut Processor,
    config_manager: &ConfigManager,
    options: &ProcessingOptions,
) -> Result<()> {
    let mapped_path = path.with_extension(target_ext);
    fs::rename(path, &mapped_path)?;
    let result = processor.process_file_with_config(&mapped_path, config_manager, Some(options));
    fs::rename(&mapped_path, path)?;
    match result {
        Ok(r) => {
            if r.original_content != r.processed_content {
                fs::write(path, r.processed_content)?;
                println!("    [DLP] Scrubbed: {:?}", path.file_name().unwrap_or_default());
            }
        }
        Err(e) => {
            println!("    [DLP] Skipping {:?}: {e}", path.file_name().unwrap_or_default());
        }
    }
    Ok(())
}

fn process_normal(
    path: &Path,
    processor: &mut Processor,
    config_manager: &ConfigManager,
    options: &ProcessingOptions,
) -> Result<()> {
    match processor.process_file_with_config(path, config_manager, Some(options)) {
        Ok(result) => {
            if result.original_content != result.processed_content {
                fs::write(path, result.processed_content)?;
                println!("    [DLP] Scrubbed: {:?}", path.file_name().unwrap_or_default());
            }
        }
        Err(e) => {
            println!("    [DLP] Skipping {:?}: {e}", path.file_name().unwrap_or_default());
        }
    }
    Ok(())
}

struct VisitContext<'a> {
    processor: &'a mut Processor,
    config_manager: &'a ConfigManager,
    options: &'a ProcessingOptions,
    ext_map: &'a HashMap<String, String>,
    extra_extensions: &'a [String],
    ignore: &'a IgnoreRules,
    base_dir: &'a Path,
}

fn visit_dirs(current_dir: &Path, ctx: &mut VisitContext<'_>) -> Result<()> {
    if !current_dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(current_dir)? {
        let entry = entry?;
        let path = entry.path();

        let relative = path.strip_prefix(ctx.base_dir).unwrap_or(&path);

        if path.is_dir() {
            let dir_name = relative.file_name().unwrap_or_default();
            if dir_name == ".git" || ctx.ignore.is_ignored(relative) {
                continue;
            }
            visit_dirs(&path, ctx)?;
        } else if let Some(ext_os) = path.extension() {
            let ext = ext_os.to_string_lossy().to_lowercase();
            let known = VALID_EXTENSIONS.contains(&ext.as_str())
                || ctx.extra_extensions.iter().any(|e| e.as_str() == ext.as_str());
            if known || ctx.ext_map.contains_key(ext.as_str()) {
                if ctx.ignore.is_ignored(relative) {
                    continue;
                }
                if let Some(target) = ctx.ext_map.get(ext.as_str()) {
                    process_mapped(&path, target, ctx.processor, ctx.config_manager, ctx.options)?;
                } else {
                    process_normal(&path, ctx.processor, ctx.config_manager, ctx.options)?;
                }
            }
        }
    }
    Ok(())
}

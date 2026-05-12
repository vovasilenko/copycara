//! DLP comment scrubbing via uncomment (tree-sitter AST).
//!
//! Walks a directory tree and removes comments from recognised source files.
//! Supports extension mapping for unsupported file types (rename-trick).

use crate::config::CopycaraConfig;
use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use uncomment::config::{Config, ConfigManager};
use uncomment::processor::{ProcessingOptions, Processor};

const VALID_EXTENSIONS: &[&str] =
    &["py", "rs", "js", "ts", "cpp", "c", "h", "hpp", "java", "go", "cs", "rb", "sh"];

pub fn apply_dlp_cleanup(dir: &Path) -> Result<()> {
    println!("  [Copycara Engine] Applying uncomment (tree-sitter AST)...");

    let cfg = CopycaraConfig::load();
    let (remove_todo, remove_fixme, remove_doc) = match cfg.cleanup.mode.as_str() {
        "smart" => (false, false, false),
        _ => (true, true, true),
    };
    let ext_map = cfg.cleanup.extension_map.clone();

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

    visit_dirs(dir, &mut processor, &config_manager, &options, &ext_map)?;
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

fn visit_dirs(
    current_dir: &Path,
    processor: &mut Processor,
    config_manager: &ConfigManager,
    options: &ProcessingOptions,
    ext_map: &HashMap<String, String>,
) -> Result<()> {
    if !current_dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(current_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            if path.file_name().unwrap_or_default() != ".git" {
                visit_dirs(&path, processor, config_manager, options, ext_map)?;
            }
        } else if let Some(ext_os) = path.extension() {
            let ext = ext_os.to_string_lossy().to_lowercase();
            if VALID_EXTENSIONS.contains(&ext.as_str()) || ext_map.contains_key(ext.as_str()) {
                if let Some(target) = ext_map.get(ext.as_str()) {
                    process_mapped(&path, target, processor, config_manager, options)?;
                } else {
                    process_normal(&path, processor, config_manager, options)?;
                }
            }
        }
    }
    Ok(())
}

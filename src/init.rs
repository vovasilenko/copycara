//! Init and uninstall commands.
//!
//! `init_command` prepares a repository for Copycara DLP processing:
//! autofix for empty repos, worktree, config with auto-detected remotes,
//! refspecs, hooks, upstream config, git config hints, initial shadow commit.
//! `uninstall_command` reverses everything.

use crate::config::CopycaraConfig;
use crate::git::run_git;
use crate::hooks;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

fn remote_exists(remote_name: &str) -> bool {
    run_git(&["remote", "get-url", remote_name], None).is_ok()
}

fn detected_config_content() -> String {
    let public =
        if remote_exists("origin") { r#"public = ["origin"]"# } else { r"public = []" };
    let private =
        if remote_exists("private") { r#"private = ["private"]"# } else { r"private = []" };

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
__PUBLIC__

# Remote-ы, в которые уходит ГРЯЗНЫЙ бэкап (оригинальный код + git notes)
__PRIVATE__

[push]
# Использовать --force-with-lease при copycara push --force
force_with_lease = true

[hooks]
install_pre_push = true
install_post_checkout = true
"#
    .replace("__PUBLIC__", public)
    .replace("__PRIVATE__", private)
}

fn setup_refspecs(cfg: &CopycaraConfig) {
    for remote in &cfg.remotes.public {
        let _ = run_git(
            &[
                "config",
                "--add",
                &format!("remote.{remote}.push"),
                "refs/copycara/heads/*:refs/heads/*",
            ],
            None,
        );
    }
    for remote in &cfg.remotes.private {
        let _ = run_git(
            &["config", "--add", &format!("remote.{remote}.push"), "refs/heads/*:refs/heads/*"],
            None,
        );
        let _ = run_git(
            &[
                "config",
                "--add",
                &format!("remote.{remote}.push"),
                "refs/notes/copycara-map:refs/notes/copycara-map",
            ],
            None,
        );
    }
}

fn remove_refspecs(cfg: &CopycaraConfig) {
    for remote in &cfg.remotes.public {
        let _ = run_git(&["config", "--unset-all", &format!("remote.{remote}.push")], None);
    }
    for remote in &cfg.remotes.private {
        let _ = run_git(&["config", "--unset-all", &format!("remote.{remote}.push")], None);
    }
}

pub fn init_command() -> Result<()> {
    println!("[Copycara Init] Starting repository initialization...");

    if !Path::new(".git").exists() {
        anyhow::bail!(
            "No .git directory found! Please run 'copycara init' inside a git repository."
        );
    }

    if run_git(&["rev-parse", "HEAD"], None).is_err() {
        println!("[Copycara Init] 0. Creating initial commit (empty repository detected)...");
        run_git(&["commit", "--allow-empty", "-m", "chore: copycara initialization"], None)?;
    }

    println!("[Copycara Init] 1. Creating shadow worktree and config with detected remotes...");
    if Path::new(".copycara/mirror").exists() {
        println!("  Worktree already exists. Skipping.");
    } else {
        fs::create_dir_all(".copycara")?;
        fs::write(".copycara/.gitignore", "*\n!config.toml\n")?;
        if !Path::new(".copycara/config.toml").exists() {
            fs::write(".copycara/config.toml", detected_config_content())?;
        }
        run_git(&["worktree", "add", "-q", "--detach", ".copycara/mirror"], None)?;
    }

    let cfg = CopycaraConfig::load();
    let public = cfg.remotes.public.join(", ");
    let private = cfg.remotes.private.join(", ");
    println!("  Public remotes (clean code):   [{public}]");
    println!("  Private remotes (dirty backup): [{private}]");

    println!("[Copycara Init] 2. Configuring Git refspecs for transparent routing...");
    setup_refspecs(&cfg);

    println!("[Copycara Init] 3. Installing Copycara hooks...");
    let hooks_dir = Path::new(".git/hooks");
    let exe_path = std::env::current_exe().context("Failed to get current executable path")?;
    let exe_str = exe_path.to_str().context("Executable path contains invalid UTF-8")?;
    hooks::install_hooks(hooks_dir, exe_str, &cfg)?;

    println!("[Copycara Init] 4. Configuring branch upstream tracking...");
    let current_branch = run_git(&["rev-parse", "--abbrev-ref", "HEAD"], None)?.trim().to_string();
    if remote_exists("private") {
        let _ = run_git(
            &["branch", "--set-upstream-to", &format!("private/{current_branch}"), &current_branch],
            None,
        );
        println!("  Branch '{current_branch}' now tracks 'private/{current_branch}'");
    } else {
        let current_remote = run_git(&["config", &format!("branch.{current_branch}.remote")], None)
            .unwrap_or_default()
            .trim()
            .to_string();
        if current_remote == "origin" {
            let _ =
                run_git(&["config", "--unset", &format!("branch.{current_branch}.remote")], None);
            let _ =
                run_git(&["config", "--unset", &format!("branch.{current_branch}.merge")], None);
            println!("  Untracked '{current_branch}' from 'origin' (hashes differ due to DLP)");
        }
    }

    println!("[Copycara Init] 5. Writing git config hints for AI agents...");
    let _ = run_git(&["config", "--local", "copycara.enabled", "true"], None);
    let _ = run_git(&["config", "--local", "copycara.sync-command", "copycara sync"], None);
    let _ = run_git(&["config", "--local", "copycara.push-command", "copycara push"], None);

    println!("[Copycara Init] 6. Creating initial shadow commit for current branch...");
    if let Err(e) = crate::commit::process_commit_command("HEAD") {
        eprintln!("  [Warning] Could not create initial shadow commit: {e}");
        eprintln!("  The first 'copycara push' may fail. Push with '--force' to bootstrap.");
    }

    println!("\n[Success] Repository initialized with Copycara DLP engine!");
    println!("Hooks point to: {exe_str}");

    Ok(())
}

pub fn uninstall_command() -> Result<()> {
    println!("[Copycara Uninstall] Removing DLP engine from repository...");

    if !Path::new(".git").exists() {
        anyhow::bail!(
            "No .git directory found! Please run 'copycara uninstall' inside a git repository."
        );
    }

    let cfg = CopycaraConfig::load();

    println!("[Copycara Uninstall] 1. Removing Git refspecs routing...");
    remove_refspecs(&cfg);

    println!("[Copycara Uninstall] 2. Removing shadow worktree and .copycara directory...");
    let _ = run_git(&["worktree", "remove", "-f", ".copycara/mirror"], None);
    if let Err(e) = fs::remove_dir_all(".copycara") {
        eprintln!("  [Warning] Failed to remove .copycara: {e}");
    }

    println!("[Copycara Uninstall] 3. Removing Git hooks...");
    hooks::remove_hooks(Path::new(".git/hooks"));

    println!("[Copycara Uninstall] 4. Removing git config hints...");
    let _ = run_git(&["config", "--local", "--unset", "copycara.enabled"], None);
    let _ = run_git(&["config", "--local", "--unset", "copycara.sync-command"], None);
    let _ = run_git(&["config", "--local", "--unset", "copycara.push-command"], None);

    println!("\n[Success] Copycara DLP engine has been completely removed from this repository.");
    println!("Standard Git behavior is fully restored.");
    Ok(())
}

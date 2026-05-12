//! Init and uninstall commands.
//!
//! `init_command` prepares a repository for Copycara DLP processing:
//! autofix for empty repos, refspecs, worktree, hooks, upstream config, git config hints.
//! `uninstall_command` reverses everything.

use crate::config::CopycaraConfig;
use crate::git::run_git;
use crate::hooks;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

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

    println!("[Copycara Init] 1. Configuring Git refspecs for transparent routing...");
    let _ = run_git(
        &["config", "--add", "remote.origin.push", "refs/copycara/heads/*:refs/heads/*"],
        None,
    );
    let _ = run_git(&["config", "--add", "remote.private.push", "refs/heads/*:refs/heads/*"], None);
    let _ = run_git(
        &[
            "config",
            "--add",
            "remote.private.push",
            "refs/notes/copycara-map:refs/notes/copycara-map",
        ],
        None,
    );

    println!("[Copycara Init] 2. Creating shadow worktree in .copycara/mirror...");
    if Path::new(".copycara/mirror").exists() {
        println!("  Worktree already exists. Skipping.");
    } else {
        fs::create_dir_all(".copycara")?;
        fs::write(".copycara/.gitignore", "*\n")?;
        if !Path::new(".copycara/config.toml").exists() {
            fs::write(".copycara/config.toml", CopycaraConfig::default_config_content())?;
        }
        run_git(&["worktree", "add", "-q", "--detach", ".copycara/mirror"], None)?;
    }

    println!("[Copycara Init] 3. Installing Copycara hooks...");
    let hooks_dir = Path::new(".git/hooks");
    let exe_path = std::env::current_exe().context("Failed to get current executable path")?;
    let exe_str = exe_path.to_str().context("Executable path contains invalid UTF-8")?;
    let cfg = CopycaraConfig::load();
    hooks::install_hooks(hooks_dir, exe_str, &cfg)?;

    println!("[Copycara Init] 4. Configuring branch upstream tracking...");
    let current_branch = run_git(&["rev-parse", "--abbrev-ref", "HEAD"], None)?.trim().to_string();
    if run_git(&["remote", "get-url", "private"], None).is_ok() {
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

    println!("[Copycara Uninstall] 1. Removing Git refspecs routing...");
    let _ = run_git(&["config", "--unset-all", "remote.origin.push"], None);
    let _ = run_git(&["config", "--unset-all", "remote.private.push"], None);

    println!("[Copycara Uninstall] 2. Removing shadow worktree and .copycara directory...");
    let _ = run_git(&["worktree", "remove", "-f", ".copycara/mirror"], None);
    if let Err(e) = fs::remove_dir_all(".copycara") {
        eprintln!("  [Warning] Failed to remove .copycara: {e}");
    }

    println!("[Copycara Uninstall] 3. Removing Git hooks...");
    hooks::remove_hooks(Path::new(".git/hooks"));

    println!("\n[Success] Copycara DLP engine has been completely removed from this repository.");
    println!("Standard Git behavior is fully restored.");
    Ok(())
}

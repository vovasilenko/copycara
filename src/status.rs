//! `copycara status` — show health status of Copycara in the current repository.

use crate::config::CopycaraConfig;
use crate::git::run_git;
use anyhow::Result;
use std::path::Path;

fn print_status(icon: &str, msg: &str) {
    println!("  {icon}  {msg}");
}

fn remote_exists(name: &str) -> bool {
    run_git(&["remote", "get-url", name], None).is_ok()
}

#[allow(clippy::too_many_lines)]
pub fn status_command() -> Result<()> {
    if !Path::new(".git").exists() {
        anyhow::bail!("No .git directory found — not a git repository.");
    }

    println!("=== Copycara Status ===");

    if !Path::new(".copycara/config.toml").exists() || !Path::new(".copycara/mirror").exists() {
        print_status("✗", "Not initialized. Run 'copycara init'.");
        return Ok(());
    }
    print_status("✓", "Initialized");

    let cfg = CopycaraConfig::load();
    let pub_remotes: Vec<_> =
        cfg.remotes.public.iter().filter(|r| remote_exists(r)).cloned().collect();
    let priv_remotes: Vec<_> =
        cfg.remotes.private.iter().filter(|r| remote_exists(r)).cloned().collect();

    let public_list = if pub_remotes.is_empty() { "(none)".into() } else { pub_remotes.join(", ") };
    let private_list =
        if priv_remotes.is_empty() { "(none)".into() } else { priv_remotes.join(", ") };

    print_status("✓", &format!("Public remotes:  [{public_list}]"));
    if pub_remotes.len() < cfg.remotes.public.len() {
        let missing: Vec<_> = cfg.remotes.public.iter().filter(|r| !remote_exists(r)).collect();
        print_status(
            "⚠",
            &format!(
                "Missing: {}",
                missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
            ),
        );
    }

    print_status("✓", &format!("Private remotes: [{private_list}]"));
    if priv_remotes.len() < cfg.remotes.private.len() {
        let missing: Vec<_> = cfg.remotes.private.iter().filter(|r| !remote_exists(r)).collect();
        print_status(
            "⚠",
            &format!(
                "Missing: {}",
                missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
            ),
        );
    }

    print_status("✓", &format!("Cleanup mode:    {}", cfg.cleanup.mode));
    let ci = if cfg.push.allow_empty_diff { "enabled" } else { "disabled" };
    print_status("✓", &format!("Empty-diff CI:   {ci}"));

    // Shadow ref
    let branch = run_git(&["rev-parse", "--abbrev-ref", "HEAD"], None)?.trim().to_string();
    let shadow_ref = format!("refs/copycara/heads/{branch}");

    if let Ok(shadow_sha) = run_git(&["rev-parse", &shadow_ref], None) {
        let shadow_sha = shadow_sha.trim();
        if shadow_sha.is_empty() {
            print_status("✗", &format!("Shadow ref for '{branch}' is missing"));
        } else {
            print_status("✓", &format!("Shadow ref exists for '{branch}': {shadow_sha:.7}"));
        }
    } else {
        print_status("✗", &format!("No shadow ref for '{branch}'. Run 'copycara init' or commit."));
    }

    // Hooks
    let hooks_dir = Path::new(".git/hooks");
    let hook_names = ["post-commit", "post-merge", "post-rewrite", "pre-push", "post-checkout"];
    let mut missing: Vec<&str> = Vec::new();

    for name in &hook_names {
        let path = hooks_dir.join(name);
        if !path.exists() {
            missing.push(name);
            continue;
        }
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        if !content.contains("Copycara") && !content.contains("copycara") {
            missing.push(name);
        }
    }

    if missing.is_empty() {
        print_status("✓", "All 5 hooks installed");
    } else {
        print_status("✗", &format!("Missing hooks: {}", missing.join(", ")));
        println!("     Run 'copycara init' to reinstall.");
    }

    // .ignore
    if Path::new(".copycara/.ignore").exists() {
        print_status("✓", ".copycara/.ignore exists");
    } else {
        print_status("⚠", ".copycara/.ignore missing (using defaults)");
    }

    // Git config hints
    if run_git(&["config", "--local", "copycara.enabled"], None).is_ok() {
        print_status("✓", "Git config hints present (copycara.*)");
    } else {
        print_status("✗", "Git config hints missing. Run 'copycara init'.");
    }

    println!();
    Ok(())
}

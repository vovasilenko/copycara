//! Reverse smudge — synchronise workspace with remote changes.
//!
//! Fetches clean code from `origin`, merges into `.copycara/mirror`,
//! extracts diff, and applies it to the dirty workspace via 3-way merge.

use crate::git::run_git;
use anyhow::Result;
use std::fs;
use std::path::Path;

const SYNC_STATE_FILE: &str = ".copycara/SYNC_IN_PROGRESS";
const PATCH_FILE: &str = ".copycara/patch.diff";

pub fn sync_continue() -> Result<()> {
    if !Path::new(SYNC_STATE_FILE).exists() {
        anyhow::bail!(
            "No sync in progress. Run 'copycara sync' without --continue to start a new sync."
        );
    }

    let new_shadow_hash = fs::read_to_string(SYNC_STATE_FILE)?.trim().to_string();
    println!(
        "[Copycara Sync] Resuming sync process for shadow commit {}...",
        &new_shadow_hash[..7]
    );

    let unmerged = run_git(&["ls-files", "-u"], None)?;
    if !unmerged.trim().is_empty() {
        anyhow::bail!(
            "You still have unresolved conflicts. Please fix them, run 'git add', and try again."
        );
    }

    println!("[Copycara Sync] Committing resolved changes and updating notes map...");
    let commit_msg =
        run_git(&["log", "-1", "--pretty=%B", &new_shadow_hash], Some(".copycara/mirror"))?
            .trim()
            .to_string();

    let commit_res = run_git(
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "-q",
            "-m",
            &format!("Merge remote sync:\n\n{commit_msg}"),
        ],
        None,
    );
    if let Err(e) = commit_res {
        anyhow::bail!(
            "Failed to commit. Did you run 'git add' on the resolved files?\nError: {e:?}"
        );
    }

    let new_workspace_hash = run_git(&["rev-parse", "HEAD"], None)?.trim().to_string();
    let _ = run_git(
        &[
            "notes",
            "--ref",
            "copycara-map",
            "add",
            "-f",
            "-m",
            &new_shadow_hash,
            &new_workspace_hash,
        ],
        None,
    );

    let _ = fs::remove_file(SYNC_STATE_FILE);
    let _ = fs::remove_file(PATCH_FILE);

    println!("\n[Success] Workspace synced! Reverse mapping created:");
    println!("Original: {} -> Shadow: {}", &new_workspace_hash[..7], &new_shadow_hash[..7]);

    Ok(())
}

pub fn sync_start() -> Result<()> {
    if Path::new(SYNC_STATE_FILE).exists() {
        anyhow::bail!(
            "A sync is already in progress! Please resolve conflicts and run 'git commit'."
        );
    }

    println!("[Copycara Sync] Starting reverse patching process...");

    let current_branch = run_git(&["branch", "--show-current"], None)?.trim().to_string();
    if current_branch.is_empty() {
        anyhow::bail!("Not on any branch. Please checkout a branch first.");
    }
    println!("[Copycara Sync] Current branch: {current_branch}");

    println!("[Copycara Sync] 1. Fetching clean code from origin...");
    run_git(&["fetch", "origin", &current_branch], None)?;

    println!("[Copycara Sync] 2. Syncing shadow graph in .copycara/mirror...");
    let old_shadow_hash =
        run_git(&["rev-parse", &format!("refs/copycara/heads/{current_branch}")], None)?
            .trim()
            .to_string();

    run_git(&["checkout", "-q", &old_shadow_hash], Some(".copycara/mirror"))?;

    let merge_res = run_git(
        &["merge", "--no-edit", &format!("origin/{current_branch}")],
        Some(".copycara/mirror"),
    );
    if merge_res.is_err() {
        let _ = run_git(&["merge", "--abort"], Some(".copycara/mirror"));
        anyhow::bail!("Merge conflict in shadow graph! Aborting.");
    }

    let new_shadow_hash =
        run_git(&["rev-parse", "HEAD"], Some(".copycara/mirror"))?.trim().to_string();
    run_git(
        &["update-ref", &format!("refs/copycara/heads/{current_branch}"), &new_shadow_hash],
        None,
    )?;

    println!("[Copycara Sync] 3. Extracting clean patch...");
    let patch_content =
        run_git(&["diff", &old_shadow_hash, &new_shadow_hash], Some(".copycara/mirror"))?;

    if patch_content.trim().is_empty() {
        println!("[Copycara Sync] No new changes to apply. Up to date.");
        return Ok(());
    }

    fs::write(PATCH_FILE, &patch_content)?;

    println!("[Copycara Sync] 4. Applying patch to workspace (3-way)...");
    let apply_res = run_git(&["apply", "--3way", PATCH_FILE], None);

    if let Err(e) = apply_res {
        fs::write(SYNC_STATE_FILE, &new_shadow_hash)?;
        println!("\n[!] Conflict detected during 3-way merge!");
        println!("Please resolve conflicts in your working directory and run 'git add'.");
        println!(
            "After that, simply run 'git commit -m \"Resolve sync conflict\"' to finalize the sync automatically."
        );
        return Err(e);
    }

    println!("[Copycara Sync] 5. Committing changes and updating notes map...");
    let commit_msg =
        run_git(&["log", "-1", "--pretty=%B", &new_shadow_hash], Some(".copycara/mirror"))?
            .trim()
            .to_string();

    run_git(&["add", "."], None)?;
    run_git(
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "-q",
            "-m",
            &format!("Merge remote sync:\n\n{commit_msg}"),
        ],
        None,
    )?;

    let new_workspace_hash = run_git(&["rev-parse", "HEAD"], None)?.trim().to_string();
    let _ = run_git(
        &[
            "notes",
            "--ref",
            "copycara-map",
            "add",
            "-f",
            "-m",
            &new_shadow_hash,
            &new_workspace_hash,
        ],
        None,
    );

    let _ = fs::remove_file(PATCH_FILE);

    println!("\n[Success] Workspace synced! Reverse mapping created:");
    println!("Original: {} -> Shadow: {}", &new_workspace_hash[..7], &new_shadow_hash[..7]);

    Ok(())
}

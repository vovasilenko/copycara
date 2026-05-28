//! Commit processing (forward smudge).
//!
//! Called by post-commit / post-merge / post-rewrite hooks.
//! Creates shadow commits in `.copycara/mirror` and maps them to originals via git notes.

use crate::dlp;
use crate::git::{run_git, run_git_with_stdin};
use crate::ignore::IgnoreRules;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

const SYNC_STATE_FILE: &str = ".copycara/SYNC_IN_PROGRESS";
const PATCH_FILE: &str = ".copycara/patch.diff";

pub fn process_commit_command(target_hash: &str) -> Result<()> {
    if Path::new(SYNC_STATE_FILE).exists() {
        return finalize_sync(target_hash);
    }

    let original_hash = run_git(&["rev-parse", target_hash], None)?.trim().to_string();
    let original_msg =
        run_git(&["log", "-1", "--pretty=%B", &original_hash], None)?.trim().to_string();

    let mut current_branch =
        run_git(&["branch", "--contains", &original_hash, "--format=%(refname:short)"], None)?
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();

    if current_branch.is_empty() || current_branch == "HEAD" {
        current_branch = run_git(&["rev-parse", "--abbrev-ref", "HEAD"], None)?.trim().to_string();
    }

    println!(
        "\n[Copycara Engine] Processing commit {} on branch '{current_branch}'...",
        &original_hash[..7]
    );

    let original_parent = run_git(&["rev-parse", &format!("{target_hash}^")], None)
        .unwrap_or_default()
        .trim()
        .to_string();
    let shadow_parent = if original_parent.is_empty() {
        String::new()
    } else {
        run_git(&["notes", "--ref", "copycara-map", "show", &original_parent], None)
            .unwrap_or_default()
            .trim()
            .to_string()
    };

    let mirror_dir = ".copycara/mirror";

    if shadow_parent.is_empty() {
        let _ = run_git(&["checkout", "--orphan", "copycara-main"], Some(mirror_dir));
        let _ = run_git(&["rm", "-rf", "."], Some(mirror_dir));
    } else {
        run_git(&["checkout", "-q", &shadow_parent], Some(mirror_dir))?;
    }

    // Use read-tree --reset -u to sync the mirror's index and working tree to
    // exactly match the dirty commit: handles additions, modifications, AND deletions.
    // Unlike `git checkout <hash> -- .`, read-tree -u also removes files that
    // were deleted in the dirty commit from both the index and working directory.
    run_git(&["read-tree", "--reset", "-u", &original_hash], Some(mirror_dir))?;

    dlp::apply_dlp_cleanup(Path::new(mirror_dir)).context("Failed to apply uncomment library")?;

    // Stage all DLP-cleaned files, then remove ignored paths from the tree.
    // Uses .copycara/.ignore (default: /.copycara/) to keep Copycara itself invisible.
    run_git(&["add", "."], Some(mirror_dir))?;

    let ignore = IgnoreRules::load();
    let staged = run_git(&["diff", "--cached", "--name-only"], Some(mirror_dir))?;
    for file in staged.lines() {
        let file = file.trim();
        if !file.is_empty() && ignore.is_ignored(Path::new(file)) {
            let _ = run_git(&["rm", "--cached", "--quiet", file], Some(mirror_dir));
        }
    }

    let is_clean = run_git(&["diff", "--cached", "--quiet"], Some(mirror_dir)).is_ok();

    if is_clean {
        println!("[Copycara Engine] Diff is empty. Dropping commit from shadow history.");
        let _ = run_git(
            &["notes", "--ref", "copycara-map", "add", "-f", "-m", &shadow_parent, &original_hash],
            None,
        );
        if current_branch != "HEAD" {
            let _ = run_git(
                &["update-ref", &format!("refs/copycara/heads/{current_branch}"), &shadow_parent],
                None,
            );
        }
    } else {
        let parents = run_git(&["log", "-1", "--pretty=%P", &original_hash], None)?;
        let mut parent_args = Vec::new();
        for p in parents.split_whitespace() {
            if let Ok(sp) = run_git(&["notes", "--ref", "copycara-map", "show", p], None) {
                let sp = sp.trim().to_string();
                if !sp.is_empty() {
                    parent_args.push("-p".to_string());
                    parent_args.push(sp);
                }
            }
        }

        let tree_hash = run_git(&["write-tree"], Some(mirror_dir))?.trim().to_string();
        let mut commit_args = vec!["commit-tree", &tree_hash];
        let refs: Vec<&str> = parent_args.iter().map(String::as_str).collect();
        commit_args.extend(refs);

        let shadow_hash =
            run_git_with_stdin(&commit_args, Some(mirror_dir), &original_msg)?.trim().to_string();

        let _ = run_git(
            &["notes", "--ref", "copycara-map", "add", "-f", "-m", &shadow_hash, &original_hash],
            None,
        );
        if current_branch != "HEAD" {
            run_git(
                &["update-ref", &format!("refs/copycara/heads/{current_branch}"), &shadow_hash],
                None,
            )?;
        }
        println!("[Copycara Engine] Shadow commit created: {}.", &shadow_hash[..7]);
    }

    Ok(())
}

fn finalize_sync(target_hash: &str) -> Result<()> {
    let original_hash = run_git(&["rev-parse", target_hash], None)?.trim().to_string();
    println!("\n[Copycara Engine] Sync state detected. Finalizing reverse patch resolution...");

    let shadow_hash = fs::read_to_string(SYNC_STATE_FILE)?.trim().to_string();

    let _ = run_git(
        &["notes", "--ref", "copycara-map", "add", "-f", "-m", &shadow_hash, &original_hash],
        None,
    );

    let _ = fs::remove_file(SYNC_STATE_FILE);
    let _ = fs::remove_file(PATCH_FILE);

    println!(
        "[Success] Workspace synced automatically via post-commit hook! Reverse mapping created:"
    );
    println!("Original: {} -> Shadow: {}", &original_hash[..7], &shadow_hash[..7]);

    Ok(())
}

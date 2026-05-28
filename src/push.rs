//! `copycara push` — safe push of clean code to public remotes and dirty backup to private remotes.

use crate::config::CopycaraConfig;
use crate::git::run_git;
use anyhow::{bail, Context, Result};

pub fn push_command(force: bool, no_private: bool) -> Result<()> {
    let cfg = CopycaraConfig::load();
    let branch = run_git(&["rev-parse", "--abbrev-ref", "HEAD"], None)?.trim().to_string();
    let shadow_refspec = format!("refs/copycara/heads/{branch}:refs/heads/{branch}");
    let dirty_refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    let notes_refspec = "refs/notes/copycara-map:refs/notes/copycara-map";
    let force_flag = if cfg.push.force_with_lease { "--force-with-lease" } else { "--force" };

    // Push clean shadow refs to all public remotes
    for remote in &cfg.remotes.public {
        if run_git(&["remote", "get-url", remote], None).is_err() {
            println!("[Copycara Push] Skipping public remote '{remote}' — not configured.");
            continue;
        }
        println!("[Copycara Push] Pushing clean code to {remote} ({branch})...");
        let result = if force {
            run_git(&["push", force_flag, remote, &shadow_refspec], None)
        } else {
            run_git(&["push", remote, &shadow_refspec], None)
        };

        if let Err(e) = result {
            let msg = format!("{e}");
            if msg.contains("non-fast-forward") || msg.contains("[rejected]") {
                bail!(
                    "Push rejected — the shadow ref for '{branch}' has no common ancestor with {remote}.\n\
                     This happens after 'copycara init' on a branch that already has history on {remote}.\n\
                     \n\
                     Fix: copycara push --force"
                );
            }
            return Err(e).context(format!("Failed to push shadow ref to {remote}"));
        }
    }

    // Push dirty backup to all private remotes.
    // Push only the current branch + notes map, not ALL branches (avoids
    // non-fast-forward when other branches are behind their remote counterparts).
    if !no_private {
        for remote in &cfg.remotes.private {
            if run_git(&["remote", "get-url", remote], None).is_err() {
                println!("[Copycara Push] Skipping private remote '{remote}' — not configured.");
                continue;
            }
            println!("[Copycara Push] Pushing private backup to {remote} ({branch})...");
            let result = if force {
                run_git(&["push", force_flag, remote, &dirty_refspec, notes_refspec], None)
            } else {
                run_git(&["push", remote, &dirty_refspec, notes_refspec], None)
            };

            if let Err(e) = result {
                let msg = format!("{e}");
                // If notes map failed (e.g. remote has diverged notes), retry without notes
                if msg.contains("[rejected]") && msg.contains("notes") {
                    eprintln!("  [Warning] copycara-map notes rejected on {remote}, retrying branch only...");
                    let branch_result = if force {
                        run_git(&["push", force_flag, remote, &dirty_refspec], None)
                    } else {
                        run_git(&["push", remote, &dirty_refspec], None)
                    };
                    if let Err(be) = branch_result {
                        return Err(be).context(format!("Failed to push dirty backup to {remote}"));
                    }
                } else {
                    return Err(e).context(format!("Failed to push dirty backup to {remote}"));
                }
            }
        }
    }

    println!("[Copycara Push] Done.");
    Ok(())
}

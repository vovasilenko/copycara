//! `copycara push` — safe push of clean code to origin and dirty backup to private.

use crate::config::CopycaraConfig;
use crate::git::run_git;
use anyhow::{Context, Result};

pub fn push_command(force: bool, no_private: bool) -> Result<()> {
    let cfg = CopycaraConfig::load();
    let branch = run_git(&["rev-parse", "--abbrev-ref", "HEAD"], None)?.trim().to_string();
    let shadow_refspec = format!("refs/copycara/heads/{branch}:refs/heads/{branch}");

    println!("[Copycara Push] Pushing clean code to origin ({branch})...");

    if force {
        let flag = if cfg.push.force_with_lease { "--force-with-lease" } else { "--force" };
        run_git(&["push", flag, "origin", &shadow_refspec], None)
            .context("Failed to push shadow ref to origin")?;
    } else {
        let result = run_git(&["push", "origin", &shadow_refspec], None);
        if let Err(e) = result {
            let msg = format!("{e}");
            if msg.contains("non-fast-forward") || msg.contains("[rejected]") {
                anyhow::bail!(
                    "Push rejected — the shadow ref for '{branch}' has no common ancestor with origin.\n\
                     This happens after 'copycara init' on a branch that already has history on origin.\n\
                     \n\
                     Fix: copycara push --force"
                );
            }
            return Err(e);
        }
    }

    if !no_private && cfg.push.auto_push_private {
        println!("[Copycara Push] Pushing private backup...");
        if run_git(&["remote", "get-url", "private"], None).is_ok() {
            run_git(&["push", "private"], None)
                .context("Failed to push private backup")?;
        } else {
            println!("  [skip] No 'private' remote configured.");
        }
    }

    println!("[Copycara Push] Done.");
    Ok(())
}

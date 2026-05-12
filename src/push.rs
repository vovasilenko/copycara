//! `copycara push` — safe push of clean code to origin and dirty backup to private.

use crate::git::run_git;
use anyhow::Result;

pub fn push_command(force: bool, no_private: bool) -> Result<()> {
    println!("[Copycara Push] Pushing clean code to origin...");

    if force {
        let branch = run_git(&["rev-parse", "--abbrev-ref", "HEAD"], None)?.trim().to_string();
        run_git(
            &[
                "push",
                "--force-with-lease",
                "origin",
                &format!("refs/copycara/heads/{branch}:refs/heads/{branch}"),
            ],
            None,
        )?;
    } else {
        run_git(&["push", "origin"], None)?;
    }

    if !no_private {
        println!("[Copycara Push] Pushing private backup...");
        if run_git(&["remote", "get-url", "private"], None).is_ok() {
            run_git(&["push", "private"], None)?;
        } else {
            println!("  [skip] No 'private' remote configured.");
        }
    }

    println!("[Copycara Push] Done.");
    Ok(())
}

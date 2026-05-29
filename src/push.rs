//! `copycara push` — safe push of clean code to public remotes and dirty backup to private remotes.

use crate::config::CopycaraConfig;
use crate::git::run_git;
use anyhow::{Context, Result, bail};

#[allow(clippy::fn_params_excessive_bools)]
pub fn push_command(
    force: bool,
    no_private: bool,
    continue_on_error: bool,
    all_branches: bool,
) -> Result<()> {
    let cfg = CopycaraConfig::load();
    let force_flag = if cfg.push.force_with_lease { "--force-with-lease" } else { "--force" };
    let mut errors: Vec<String> = Vec::new();

    if all_branches {
        push_all_branches(&cfg, force, force_flag, continue_on_error, &mut errors)?;
    } else {
        push_current_branch(&cfg, force, force_flag, continue_on_error, &mut errors)?;
    }

    // Private backup
    if !no_private {
        push_private_all(&cfg, force, force_flag, continue_on_error, all_branches, &mut errors)?;
    }

    if !errors.is_empty() {
        eprintln!("\n  [Copycara Push] Completed with {} error(s):", errors.len());
        for err in &errors {
            eprintln!("    - {err}");
        }
    }

    println!("[Copycara Push] Done.");
    Ok(())
}

fn push_current_branch(
    cfg: &CopycaraConfig,
    force: bool,
    force_flag: &str,
    continue_on_error: bool,
    errors: &mut Vec<String>,
) -> Result<()> {
    let branch = run_git(&["rev-parse", "--abbrev-ref", "HEAD"], None)?.trim().to_string();
    let shadow_refspec = format!("refs/copycara/heads/{branch}:refs/heads/{branch}");

    push_to_public(cfg, force, force_flag, &shadow_refspec, &branch, continue_on_error, errors)
}

fn push_all_branches(
    cfg: &CopycaraConfig,
    force: bool,
    force_flag: &str,
    continue_on_error: bool,
    errors: &mut Vec<String>,
) -> Result<()> {
    for remote in &cfg.remotes.public {
        if run_git(&["remote", "get-url", remote], None).is_err() {
            println!("[Copycara Push] Skipping public remote '{remote}' — not configured.");
            continue;
        }
        println!("[Copycara Push] Pushing ALL clean branches to {remote}...");
        let result = if force {
            run_git(&["push", force_flag, remote], None)
        } else {
            run_git(&["push", remote], None)
        };

        if let Err(e) = result {
            let msg = format!("{e}");
            if msg.contains("non-fast-forward") || msg.contains("[rejected]") {
                if continue_on_error {
                    eprintln!("  [Error] {remote}: non-fast-forward. Use --force to override.");
                    errors.push(format!("{remote}: non-fast-forward"));
                    continue;
                }
                bail!(
                    "Push rejected — some shadow refs have no common ancestor with {remote}.\n\
                     This happens for branches that have not been initialised on this machine.\n\
                     \n\
                     Fix for specific branch: switch to it, run 'copycara init', then push.\n\
                     Fix for all branches:        copycara push --force --all"
                );
            }
            if continue_on_error {
                eprintln!("  [Error] {remote}: {e}");
                errors.push(format!("{remote}: {e}"));
                continue;
            }
            return Err(e).context(format!("Failed to push shadow refs to {remote}"));
        }
    }
    Ok(())
}

fn push_to_public(
    cfg: &CopycaraConfig,
    force: bool,
    force_flag: &str,
    shadow_refspec: &str,
    branch: &str,
    continue_on_error: bool,
    errors: &mut Vec<String>,
) -> Result<()> {
    for remote in &cfg.remotes.public {
        if run_git(&["remote", "get-url", remote], None).is_err() {
            println!("[Copycara Push] Skipping public remote '{remote}' — not configured.");
            continue;
        }
        println!("[Copycara Push] Pushing clean code to {remote} ({branch})...");
        let result = if force {
            run_git(&["push", force_flag, remote, shadow_refspec], None)
        } else {
            run_git(&["push", remote, shadow_refspec], None)
        };

        if let Err(e) = result {
            let msg = format!("{e}");
            if msg.contains("non-fast-forward") || msg.contains("[rejected]") {
                let err_msg = format!(
                    "Push rejected — the shadow ref for '{branch}' has no common ancestor with {remote}.\n\
                     This happens after 'copycara init' on a branch that already has history on {remote}.\n\
                     \n\
                     Fix: copycara push --force"
                );
                if continue_on_error {
                    eprintln!("  [Error] {remote}: {err_msg}");
                    errors.push(format!("{remote}: non-fast-forward"));
                    continue;
                }
                bail!(err_msg);
            }
            if continue_on_error {
                eprintln!("  [Error] {remote}: {e}");
                errors.push(format!("{remote}: {e}"));
                continue;
            }
            return Err(e).context(format!("Failed to push shadow ref to {remote}"));
        }
    }
    Ok(())
}

fn push_private_all(
    cfg: &CopycaraConfig,
    force: bool,
    force_flag: &str,
    continue_on_error: bool,
    all_branches: bool,
    errors: &mut Vec<String>,
) -> Result<()> {
    let branch = run_git(&["rev-parse", "--abbrev-ref", "HEAD"], None)
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    for remote in &cfg.remotes.private {
        if run_git(&["remote", "get-url", remote], None).is_err() {
            println!("[Copycara Push] Skipping private remote '{remote}' — not configured.");
            continue;
        }

        if all_branches {
            println!("[Copycara Push] Pushing ALL dirty branches to {remote}...");
            let result = if force {
                run_git(&["push", force_flag, remote], None)
            } else {
                run_git(&["push", remote], None)
            };
            if let Err(e) = result {
                if continue_on_error {
                    eprintln!("  [Error] {remote}: {e}");
                    errors.push(format!("{remote}: {e}"));
                } else {
                    return Err(e).context(format!("Failed to push private backup to {remote}"));
                }
            }
        } else {
            let dirty_refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
            let notes_refspec = "refs/notes/copycara-map:refs/notes/copycara-map";
            println!("[Copycara Push] Pushing private backup to {remote} ({branch})...");
            let result = if force {
                run_git(&["push", force_flag, remote, &dirty_refspec, notes_refspec], None)
            } else {
                run_git(&["push", remote, &dirty_refspec, notes_refspec], None)
            };

            if let Err(e) = result {
                let msg = format!("{e}");
                if msg.contains("[rejected]") && msg.contains("notes") {
                    eprintln!(
                        "  [Warning] copycara-map notes rejected on {remote}, retrying branch only..."
                    );
                    let branch_result = if force {
                        run_git(&["push", force_flag, remote, &dirty_refspec], None)
                    } else {
                        run_git(&["push", remote, &dirty_refspec], None)
                    };
                    if let Err(be) = branch_result {
                        if continue_on_error {
                            eprintln!("  [Error] {remote}: {be}");
                            errors.push(format!("{remote}: {be}"));
                        } else {
                            return Err(be)
                                .context(format!("Failed to push dirty backup to {remote}"));
                        }
                    }
                } else if continue_on_error {
                    eprintln!("  [Error] {remote}: {e}");
                    errors.push(format!("{remote}: private push failed"));
                } else {
                    return Err(e).context(format!("Failed to push dirty backup to {remote}"));
                }
            }
        }
    }
    Ok(())
}

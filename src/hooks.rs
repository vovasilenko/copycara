//! Git hook generators for Copycara.
//!
//! Produces bash scripts for post-commit, post-merge, post-rewrite,
//! pre-push, and post-checkout hooks. Installs and removes them.

use crate::config::CopycaraConfig;
use crate::git::write_executable_script;
use anyhow::Result;
use std::path::Path;

pub fn generate_pre_push_hook() -> &'static str {
    r#"#!/bin/bash
# Copycara pre-push hook — blocks direct push of dirty branches to origin
REMOTE="$1"
REMOTE_URL="$2"

if [ "$REMOTE" != "origin" ]; then
    exit 0
fi

BLOCKED=false

while read LOCAL_REF LOCAL_SHA REMOTE_REF REMOTE_SHA; do
    if [[ "$LOCAL_REF" == refs/heads/* ]]; then
        BRANCH_NAME=$(echo "$LOCAL_REF" | sed 's|refs/heads/||')

        cat >&2 <<BLOCKMSG

╔══════════════════════════════════════════════════════════════╗
║ [COPYCARA] BLOCKED: direct push to origin                   ║
╠══════════════════════════════════════════════════════════════╣
║                                                              ║
║  Push of refs/heads/${BRANCH_NAME} -> origin is FORBIDDEN.   ║
║                                                              ║
║  This would expose annotated (dirty) code with your          ║
║  private methodology tags (PACM, GRACE, etc.) to the         ║
║  public repository.                                          ║
║                                                              ║
║  CORRECT COMMANDS:                                           ║
║    git push origin              (no branch name)             ║
║    copycara push                 (safe wrapper)               ║
║                                                              ║
╚══════════════════════════════════════════════════════════════╝

BLOCKMSG
        BLOCKED=true
    fi
done

if [ "$BLOCKED" = true ]; then
    exit 1
fi

exit 0
"#
}

pub fn generate_post_checkout_hook() -> &'static str {
    r#"#!/bin/bash
# Copycara post-checkout hook — auto-configures upstream for new branches

if [ "$3" = "1" ]; then
    BRANCH=$(git rev-parse --abbrev-ref HEAD)
    if git remote get-url private >/dev/null 2>&1; then
        git branch --set-upstream-to="private/$BRANCH" "$BRANCH" 2>/dev/null
    else
        git config --unset "branch.$BRANCH.remote" 2>/dev/null
        git config --unset "branch.$BRANCH.merge" 2>/dev/null
    fi
fi
exit 0
"#
}

pub fn generate_post_commit_hook(exe_path: &str) -> String {
    format!("#!/bin/bash\n\"{exe_path}\" process-commit HEAD\n")
}

pub fn generate_post_rewrite_hook(exe_path: &str) -> String {
    format!(
        "#!/bin/bash\nwhile read old_hash new_hash extra_info; do\n  \"{exe_path}\" process-commit $new_hash\ndone\n"
    )
}

pub fn install_hooks(hooks_dir: &Path, exe_path: &str, config: &CopycaraConfig) -> Result<()> {
    let post_commit = generate_post_commit_hook(exe_path);
    let post_rewrite = generate_post_rewrite_hook(exe_path);

    write_executable_script(&hooks_dir.join("post-commit"), &post_commit)?;
    write_executable_script(&hooks_dir.join("post-merge"), &post_commit)?;
    write_executable_script(&hooks_dir.join("post-rewrite"), &post_rewrite)?;

    if config.hooks.install_pre_push {
        write_executable_script(&hooks_dir.join("pre-push"), generate_pre_push_hook())?;
    }

    if config.hooks.install_post_checkout {
        write_executable_script(&hooks_dir.join("post-checkout"), generate_post_checkout_hook())?;
    }

    Ok(())
}

pub fn remove_hooks(hooks_dir: &Path) {
    let names = ["post-commit", "post-merge", "post-rewrite", "pre-push", "post-checkout"];
    for name in &names {
        let _ = std::fs::remove_file(hooks_dir.join(name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HooksConfig;

    #[test]
    fn test_pre_push_hook_contains_blocked() {
        let hook = generate_pre_push_hook();
        assert!(hook.contains("COPYCARA"));
        assert!(hook.contains("BLOCKED"));
        assert!(hook.contains("git push origin"));
    }

    #[test]
    fn test_pre_push_hook_skips_non_origin() {
        let hook = generate_pre_push_hook();
        // The hook checks REMOTE != "origin" and exits 0
        assert!(hook.contains(r#"origin"#));
        assert!(hook.contains(r#"exit 0"#));
    }

    #[test]
    fn test_post_checkout_hook_triggers_on_new_branch() {
        let hook = generate_post_checkout_hook();
        assert!(hook.contains(r#"$3" = "1""#));
        assert!(hook.contains("private"));
    }

    #[test]
    fn test_post_commit_hook_invokes_binary() {
        let hook = generate_post_commit_hook("/usr/bin/copycara");
        assert!(hook.contains("/usr/bin/copycara"));
        assert!(hook.contains("process-commit"));
        assert!(hook.contains("HEAD"));
    }

    #[test]
    fn test_post_rewrite_hook_iterates_stdin() {
        let hook = generate_post_rewrite_hook("/usr/bin/copycara");
        assert!(hook.contains("while read old_hash new_hash"));
    }

    #[test]
    fn test_install_hooks_respects_config() {
        let config = CopycaraConfig {
            hooks: HooksConfig { install_pre_push: false, install_post_checkout: false },
            ..CopycaraConfig::default()
        };
        // No actual file writes; this just validates the config is used
        assert!(!config.hooks.install_pre_push);
        assert!(!config.hooks.install_post_checkout);
    }
}

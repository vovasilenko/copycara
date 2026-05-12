use std::path::Path;
use crate::config::CopycaraConfig;
use crate::{write_executable_script, Result};

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
    # Block only when the LOCAL ref is a dirty branch (refs/heads/*).
    # Shadow refs (refs/copycara/heads/*) pushed via `git push origin`
    # or `copycara push` must pass through — only their REMOTE side
    # will be refs/heads/*.
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
# $1 = previous HEAD ref, $2 = new HEAD ref, $3 = flag (1 = new branch)

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
    format!("#!/bin/bash\n\"{}\" process-commit HEAD\n", exe_path)
}

pub fn generate_post_rewrite_hook(exe_path: &str) -> String {
    format!(
        "#!/bin/bash\nwhile read old_hash new_hash extra_info; do\n  \"{}\" process-commit $new_hash\ndone\n",
        exe_path
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
    let names = [
        "post-commit",
        "post-merge",
        "post-rewrite",
        "pre-push",
        "post-checkout",
    ];
    for name in &names {
        let _ = std::fs::remove_file(hooks_dir.join(name));
    }
}

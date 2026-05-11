use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::str;
use uncomment::config::ConfigManager;
use uncomment::Processor;

const SYNC_STATE_FILE: &str = ".copycara/SYNC_IN_PROGRESS";
const PATCH_FILE: &str = ".copycara/patch.diff";

#[derive(Parser)]
#[command(name = "copycara", version = "0.1", about = "Topological Git DLP Engine")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Инициализация теневого зеркала и хуков в текущем репозитории
    Init,
    /// Обработка коммита (вызывается автоматически через git hooks)
    ProcessCommit {
        /// Хэш коммита для обработки
        target_hash: String,
    },
    /// Синхронизация с удаленным сервером (Reverse Patching)
    Sync {
        /// Продолжить синхронизацию после разрешения конфликтов (Оставлено для обратной совместимости)
        #[arg(long = "continue")]
        resume: bool,
    },
}

// ==========================================
// БЛОК GIT УТИЛИТ
// ==========================================

fn run_git(args: &[&str], dir: Option<&str>) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    
    cmd.env_remove("GIT_DIR")
       .env_remove("GIT_WORK_TREE")
       .env_remove("GIT_INDEX_FILE")
       .env_remove("GIT_PREFIX");
    
    let output: Output = cmd.output().context("Failed to execute git")?;
    
    if output.status.success() {
        Ok(str::from_utf8(&output.stdout)?.to_string())
    } else {
        let err = str::from_utf8(&output.stderr)?.trim();
        anyhow::bail!("Git command failed: {}\nStderr: {}", args.join(" "), err)
    }
}

fn run_git_with_stdin(args: &[&str], dir: Option<&str>, stdin_data: &str) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(d) = dir { cmd.current_dir(d); }
    cmd.env_remove("GIT_DIR").env_remove("GIT_WORK_TREE").env_remove("GIT_INDEX_FILE");

    cmd.stdin(Stdio::piped())
       .stdout(Stdio::piped())
       .stderr(Stdio::piped());

    let mut child = cmd.spawn().context("Failed to spawn git")?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(stdin_data.as_bytes())?;
    }

    let output = child.wait_with_output()?;
    if output.status.success() {
        Ok(str::from_utf8(&output.stdout)?.to_string())
    } else {
        let err = str::from_utf8(&output.stderr)?.trim();
        anyhow::bail!("Git command failed: {}\nStderr: {}", args.join(" "), err)
    }
}

fn write_executable_script(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content)?;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755); 
    fs::set_permissions(path, perms)?;
    Ok(())
}

// ==========================================
// БЛОК ИНИЦИАЛИЗАЦИИ
// ==========================================

fn init_command() -> Result<()> {
    println!("[Copycara Init] Starting repository initialization...");

    if !Path::new(".git").exists() {
        anyhow::bail!("No .git directory found! Please run 'copycara init' inside a git repository.");
    }

    println!("[Copycara Init] 1. Configuring Git refspecs for transparent routing...");
    run_git(&["config", "--add", "remote.origin.push", "refs/copycara/heads/*:refs/heads/*"], None).ok();
    run_git(&["config", "--add", "remote.private.push", "refs/heads/*:refs/heads/*"], None).ok();
    run_git(&["config", "--add", "remote.private.push", "refs/notes/copycara-map:refs/notes/copycara-map"], None).ok();

    println!("[Copycara Init] 2. Creating shadow worktree in .copycara/mirror...");
    if !Path::new(".copycara/mirror").exists() {
        fs::create_dir_all(".copycara")?;
        fs::write(".copycara/.gitignore", "*\n")?;
        run_git(&["worktree", "add", "-q", "--detach", ".copycara/mirror"], None)?;
    } else {
        println!("  Worktree already exists. Skipping.");
    }

    println!("[Copycara Init] 3. Installing Copycara hooks...");
    let hooks_dir = Path::new(".git/hooks");

    let exe_path = std::env::current_exe().context("Failed to get current executable path")?;
    let exe_str = exe_path.to_str().unwrap();

    let post_commit_script = format!("#!/bin/bash\n\"{}\" process-commit HEAD\n", exe_str);
    let post_rewrite_script = format!(
        "#!/bin/bash\nwhile read old_hash new_hash extra_info; do\n  \"{}\" process-commit $new_hash\ndone\n",
        exe_str
    );

    write_executable_script(&hooks_dir.join("post-commit"), &post_commit_script)?;
    write_executable_script(&hooks_dir.join("post-merge"), &post_commit_script)?;
    write_executable_script(&hooks_dir.join("post-rewrite"), &post_rewrite_script)?;

    println!("\n[Success] Repository initialized with Copycara DLP engine!");
    println!("Hooks point to: {}", exe_str);
    
    Ok(())
}

// ==========================================
// БЛОК ОЧИСТКИ (NATIVE RUST LIBRARY)
// ==========================================

fn apply_dlp_cleanup(dir: &Path) -> Result<()> {
    println!("  [Copycara Engine] Initializing tree-sitter AST parser...");

    let config_manager = ConfigManager::new(dir)
        .map_err(|e| anyhow::anyhow!("Failed to init ConfigManager: {}", e))?;
    
    let mut processor = Processor::new_with_config(&config_manager);

    fn visit_dirs(
        current_dir: &Path, 
        cm: &ConfigManager, 
        proc: &mut Processor
    ) -> Result<()> {
        if current_dir.is_dir() {
            for entry in fs::read_dir(current_dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_dir() {
                    if path.file_name().unwrap_or_default() != ".git" {
                        visit_dirs(&path, cm, proc)?;
                    }
                } else {
                    if let Ok(processed_file) = proc.process_file_with_config(&path, cm, None) {
                        if processed_file.modified {
                            let _ = fs::write(&path, &processed_file.processed_content);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    visit_dirs(dir, &config_manager, &mut processor)?;
    
    Ok(())
}

// ==========================================
// БЛОК FORWARD SMUDGE (ХУКИ)
// ==========================================

fn process_commit_command(target_hash: &str) -> Result<()> {
    // --- ИНТЕГРАЦИЯ АВТОМАТИЧЕСКОГО CONTINUE ---
    if Path::new(SYNC_STATE_FILE).exists() {
        let original_hash = run_git(&["rev-parse", target_hash], None)?.trim().to_string();
        println!("\n[Copycara Engine] Sync state detected. Finalizing reverse patch resolution...");
        
        let shadow_hash = fs::read_to_string(SYNC_STATE_FILE)?.trim().to_string();
        
        // Связываем вновь созданный коммит разрешения конфликта с чистым серверным коммитом
        run_git(&["notes", "--ref", "copycara-map", "add", "-f", "-m", &shadow_hash, &original_hash], None)?;

        // Убираем временные файлы
        let _ = fs::remove_file(SYNC_STATE_FILE);
        let _ = fs::remove_file(PATCH_FILE);

        println!("[Success] Workspace synced automatically via post-commit hook! Reverse mapping created:");
        println!("Original: {} -> Shadow: {}", &original_hash[0..7], &shadow_hash[0..7]);
        
        // Прерываем выполнение, так как это не новый пользовательский код, а завершение синхронизации
        return Ok(());
    }

    // Стандартный пайплайн очистки (Forward Smudge)
    let original_hash = run_git(&["rev-parse", target_hash], None)?.trim().to_string();
    let original_msg = run_git(&["log", "-1", "--pretty=%B", &original_hash], None)?.trim().to_string();

    let mut current_branch = run_git(&["branch", "--contains", &original_hash, "--format=%(refname:short)"], None)?
        .lines().next().unwrap_or("").trim().to_string();

    if current_branch.is_empty() || current_branch == "HEAD" {
        current_branch = run_git(&["rev-parse", "--abbrev-ref", "HEAD"], None)?.trim().to_string();
    }

    println!("\n[Copycara Engine] Processing commit {} on branch '{}'...", &original_hash[..7], current_branch);

    let original_parent = run_git(&["rev-parse", &format!("{}^", target_hash)], None).unwrap_or_default().trim().to_string();
    let mut shadow_parent = String::new();
    if !original_parent.is_empty() {
        shadow_parent = run_git(&["notes", "--ref", "copycara-map", "show", &original_parent], None).unwrap_or_default().trim().to_string();
    }

    let mirror_dir = ".copycara/mirror";

    if shadow_parent.is_empty() {
        let _ = run_git(&["checkout", "--orphan", "copycara-main"], Some(mirror_dir));
        let _ = run_git(&["rm", "-rf", "."], Some(mirror_dir));
    } else {
        run_git(&["checkout", "-q", &shadow_parent], Some(mirror_dir))?;
    }

    run_git(&["checkout", &original_hash, "--", "."], Some(mirror_dir))?;

    apply_dlp_cleanup(Path::new(mirror_dir)).context("Failed to apply uncomment library")?;

    run_git(&["add", "."], Some(mirror_dir))?;

    let is_clean = run_git(&["diff", "--cached", "--quiet"], Some(mirror_dir)).is_ok();

    if !is_clean {
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
        let refs: Vec<&str> = parent_args.iter().map(|s| s.as_str()).collect();
        commit_args.extend(refs);

        let shadow_hash = run_git_with_stdin(&commit_args, Some(mirror_dir), &original_msg)?.trim().to_string();
        
        run_git(&["notes", "--ref", "copycara-map", "add", "-f", "-m", &shadow_hash, &original_hash], None)?;
        if current_branch != "HEAD" {
            run_git(&["update-ref", &format!("refs/copycara/heads/{}", current_branch), &shadow_hash], None)?;
        }
        println!("[Copycara Engine] Shadow commit created: {}.", &shadow_hash[..7]);
    } else {
        println!("[Copycara Engine] Diff is empty. Dropping commit from shadow history.");
        run_git(&["notes", "--ref", "copycara-map", "add", "-f", "-m", &shadow_parent, &original_hash], None)?;
        if current_branch != "HEAD" {
            run_git(&["update-ref", &format!("refs/copycara/heads/{}", current_branch), &shadow_parent], None)?;
        }
    }

    Ok(())
}

// ==========================================
// БЛОК REVERSE SMUDGE (СИНХРОНИЗАЦИЯ)
// ==========================================

fn sync_continue() -> Result<()> {
    if !Path::new(SYNC_STATE_FILE).exists() {
        anyhow::bail!("No sync in progress. Run 'copycara sync' without --continue to start a new sync.");
    }

    let new_shadow_hash = fs::read_to_string(SYNC_STATE_FILE)?.trim().to_string();
    println!("[Copycara Sync] Resuming sync process for shadow commit {}...", &new_shadow_hash[..7]);

    let unmerged = run_git(&["ls-files", "-u"], None)?;
    if !unmerged.trim().is_empty() {
        anyhow::bail!("You still have unresolved conflicts. Please fix them, run 'git add', and try again.");
    }

    println!("[Copycara Sync] Committing resolved changes and updating notes map...");
    let commit_msg = run_git(&["log", "-1", "--pretty=%B", &new_shadow_hash], Some(".copycara/mirror"))?.trim().to_string();
    
    let commit_res = run_git(&["-c", "core.hooksPath=/dev/null", "commit", "-q", "-m", &format!("Merge remote sync:\n\n{}", commit_msg)], None);
    if let Err(e) = commit_res {
        anyhow::bail!("Failed to commit. Did you run 'git add' on the resolved files?\nError: {:?}", e);
    }
    
    let new_workspace_hash = run_git(&["rev-parse", "HEAD"], None)?.trim().to_string();
    run_git(&["notes", "--ref", "copycara-map", "add", "-f", "-m", &new_shadow_hash, &new_workspace_hash], None)?;

    let _ = fs::remove_file(SYNC_STATE_FILE);
    let _ = fs::remove_file(PATCH_FILE);

    println!("\n[Success] Workspace synced! Reverse mapping created:");
    println!("Original: {} -> Shadow: {}", &new_workspace_hash[0..7], &new_shadow_hash[0..7]);

    Ok(())
}

fn sync_start() -> Result<()> {
    if Path::new(SYNC_STATE_FILE).exists() {
        anyhow::bail!("A sync is already in progress! Please resolve conflicts and run 'git commit'.");
    }

    println!("[Copycara Sync] Starting reverse patching process...");

    let current_branch = run_git(&["branch", "--show-current"], None)?.trim().to_string();
    if current_branch.is_empty() {
        anyhow::bail!("Not on any branch. Please checkout a branch first.");
    }
    println!("[Copycara Sync] Current branch: {}", current_branch);

    println!("[Copycara Sync] 1. Fetching clean code from origin...");
    run_git(&["fetch", "origin", &current_branch], None)?;

    println!("[Copycara Sync] 2. Syncing shadow graph in .copycara/mirror...");
    let old_shadow_hash = run_git(&["rev-parse", &format!("refs/copycara/heads/{}", current_branch)], None)?.trim().to_string();
    
    run_git(&["checkout", "-q", &old_shadow_hash], Some(".copycara/mirror"))?;
    
    let merge_res = run_git(&["merge", "--no-edit", &format!("origin/{}", current_branch)], Some(".copycara/mirror"));
    if merge_res.is_err() {
        run_git(&["merge", "--abort"], Some(".copycara/mirror")).ok();
        anyhow::bail!("Merge conflict in shadow graph! Aborting.");
    }

    let new_shadow_hash = run_git(&["rev-parse", "HEAD"], Some(".copycara/mirror"))?.trim().to_string();
    run_git(&["update-ref", &format!("refs/copycara/heads/{}", current_branch), &new_shadow_hash], None)?;

    println!("[Copycara Sync] 3. Extracting clean patch...");
    let patch_content = run_git(&["diff", &old_shadow_hash, &new_shadow_hash], Some(".copycara/mirror"))?;
    
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
        // Обновленная инструкция для пользователя
        println!("After that, simply run 'git commit -m \"Resolve sync conflict\"' to finalize the sync automatically.");
        return Err(e);
    }

    println!("[Copycara Sync] 5. Committing changes and updating notes map...");
    let commit_msg = run_git(&["log", "-1", "--pretty=%B", &new_shadow_hash], Some(".copycara/mirror"))?.trim().to_string();
    
    run_git(&["add", "."], None)?;
    run_git(&["-c", "core.hooksPath=/dev/null", "commit", "-q", "-m", &format!("Merge remote sync:\n\n{}", commit_msg)], None)?;
    
    let new_workspace_hash = run_git(&["rev-parse", "HEAD"], None)?.trim().to_string();
    run_git(&["notes", "--ref", "copycara-map", "add", "-f", "-m", &new_shadow_hash, &new_workspace_hash], None)?;

    let _ = fs::remove_file(PATCH_FILE);

    println!("\n[Success] Workspace synced! Reverse mapping created:");
    println!("Original: {} -> Shadow: {}", &new_workspace_hash[0..7], &new_shadow_hash[0..7]);

    Ok(())
}

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Commands::Init => init_command(),
        Commands::ProcessCommit { target_hash } => process_commit_command(target_hash),
        Commands::Sync { resume } => {
            if *resume {
                sync_continue()
            } else {
                sync_start()
            }
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {:?}", e);
        std::process::exit(1);
    }
}
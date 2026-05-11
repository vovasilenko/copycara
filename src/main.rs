use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::Path;
use std::process::{Command, Output};
use std::str;

const SYNC_STATE_FILE: &str = ".copycara/SYNC_IN_PROGRESS";
const PATCH_FILE: &str = ".copycara/patch.diff";

#[derive(Parser)]
#[command(name = "copycara", version = "0.1", about = "DLP Git Wrapper")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Синхронизация с удаленным сервером (Reverse Patching)
    Sync {
        /// Продолжить синхронизацию после разрешения конфликтов
        #[arg(long = "continue")]
        resume: bool,
    },
}

/// Вспомогательная функция для запуска git команд
fn run_git(args: &[&str], dir: Option<&str>) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    
    let output: Output = cmd.output().context("Failed to execute git")?;
    
    if output.status.success() {
        Ok(str::from_utf8(&output.stdout)?.to_string())
    } else {
        let err = str::from_utf8(&output.stderr)?.trim();
        anyhow::bail!("Git command failed: {}\nStderr: {}", args.join(" "), err)
    }
}

/// Логика ПРОДОЛЖЕНИЯ синхронизации после конфликта
fn sync_continue() -> Result<()> {
    if !Path::new(SYNC_STATE_FILE).exists() {
        anyhow::bail!("No sync in progress. Run 'copycara sync' without --continue to start a new sync.");
    }

    let new_shadow_hash = std::fs::read_to_string(SYNC_STATE_FILE)?
        .trim()
        .to_string();

    println!("[Copycara Sync] Resuming sync process for shadow commit {}...", &new_shadow_hash[..7]);

    // Проверяем, остались ли неразрешенные конфликты (unmerged files)
    let unmerged = run_git(&["ls-files", "-u"], None)?;
    if !unmerged.trim().is_empty() {
        anyhow::bail!("You still have unresolved conflicts. Please fix them, run 'git add', and try again.");
    }

    println!("[Copycara Sync] Committing resolved changes and updating notes map...");
    
    // Получаем оригинальное сообщение от серверного коммита
    let commit_msg = run_git(&["log", "-1", "--pretty=%B", &new_shadow_hash], Some(".copycara/mirror"))?.trim().to_string();
    
    // Делаем коммит (пользователь уже должен был сделать git add)
    let commit_res = run_git(&["-c", "core.hooksPath=/dev/null", "commit", "-q", "-m", &format!("Merge remote sync:\n\n{}", commit_msg)], None);
    if let Err(e) = commit_res {
        anyhow::bail!("Failed to commit. Did you run 'git add' on the resolved files?\nError: {:?}", e);
    }
    
    // Связываем графы
    let new_workspace_hash = run_git(&["rev-parse", "HEAD"], None)?.trim().to_string();
    run_git(&["notes", "--ref", "copycara-map", "add", "-f", "-m", &new_shadow_hash, &new_workspace_hash], None)?;

    // Очищаем временные файлы
    let _ = std::fs::remove_file(SYNC_STATE_FILE);
    let _ = std::fs::remove_file(PATCH_FILE);

    println!("\n[Success] Workspace synced! Reverse mapping created:");
    println!("Original: {} -> Shadow: {}", &new_workspace_hash[0..7], &new_shadow_hash[0..7]);

    Ok(())
}

/// Основная логика СТАРТА синхронизации
fn sync_start() -> Result<()> {
    if Path::new(SYNC_STATE_FILE).exists() {
        anyhow::bail!("A sync is already in progress! Please resolve conflicts and run 'copycara sync --continue'.");
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

    std::fs::write(PATCH_FILE, &patch_content)?;

    println!("[Copycara Sync] 4. Applying patch to workspace (3-way)...");
    let apply_res = run_git(&["apply", "--3way", PATCH_FILE], None);
    
    if let Err(e) = apply_res {
        // СОХРАНЯЕМ СОСТОЯНИЕ В СЛУЧАЕ КОНФЛИКТА
        std::fs::write(SYNC_STATE_FILE, &new_shadow_hash)?;

        println!("\n[!] Conflict detected during 3-way merge!");
        println!("Please resolve conflicts in your working directory and run 'git add'.");
        println!("After that, run: copycara sync --continue");
        return Err(e);
    }

    println!("[Copycara Sync] 5. Committing changes and updating notes map...");
    let commit_msg = run_git(&["log", "-1", "--pretty=%B", &new_shadow_hash], Some(".copycara/mirror"))?.trim().to_string();
    
    run_git(&["add", "."], None)?;
    run_git(&["-c", "core.hooksPath=/dev/null", "commit", "-q", "-m", &format!("Merge remote sync:\n\n{}", commit_msg)], None)?;
    
    let new_workspace_hash = run_git(&["rev-parse", "HEAD"], None)?.trim().to_string();
    run_git(&["notes", "--ref", "copycara-map", "add", "-f", "-m", &new_shadow_hash, &new_workspace_hash], None)?;

    // Убираем временный патч-файл
    let _ = std::fs::remove_file(PATCH_FILE);

    println!("\n[Success] Workspace synced! Reverse mapping created:");
    println!("Original: {} -> Shadow: {}", &new_workspace_hash[0..7], &new_shadow_hash[0..7]);

    Ok(())
}

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
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
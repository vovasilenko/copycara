mod config;
mod hooks;
mod push;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::CopycaraConfig;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::str;
use uncomment::config::{Config, ConfigManager};
use uncomment::processor::{ProcessingOptions, Processor};

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
    /// Полное удаление Copycara из репозитория (восстановление стандартного Git)
    Uninstall,
    /// Обработка коммита (вызывается автоматически через git hooks)
    ProcessCommit {
        /// Хэш коммита для обработки
        target_hash: String,
    },
    /// Синхронизация с удаленным сервером (Reverse Patching)
    Sync {
        /// Продолжить синхронизацию после разрешения конфликтов
        #[arg(long = "continue")]
        resume: bool,
    },
    /// Безопасная отправка чистой версии в origin и бэкапа в private
    Push {
        /// Force push shadow refs (uses --force-with-lease)
        #[arg(long)]
        force: bool,
        /// Skip push to private remote
        #[arg(long)]
        no_private: bool,
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
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    cmd.env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");

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
// БЛОК ИНИЦИАЛИЗАЦИИ И УДАЛЕНИЯ
// ==========================================

fn init_command() -> Result<()> {
    println!("[Copycara Init] Starting repository initialization...");

    if !Path::new(".git").exists() {
        anyhow::bail!("No .git directory found! Please run 'copycara init' inside a git repository.");
    }

    // Шаг 0: Autofix пустого репозитория (Фаза 2)
    let head_exists = run_git(&["rev-parse", "HEAD"], None).is_ok();
    if !head_exists {
        println!("[Copycara Init] 0. Creating initial commit (empty repository detected)...");
        run_git(
            &[
                "commit",
                "--allow-empty",
                "-m",
                "chore: copycara initialization",
            ],
            None,
        )?;
    }

    println!("[Copycara Init] 1. Configuring Git refspecs for transparent routing...");
    run_git(
        &[
            "config",
            "--add",
            "remote.origin.push",
            "refs/copycara/heads/*:refs/heads/*",
        ],
        None,
    )
    .ok();
    run_git(
        &[
            "config",
            "--add",
            "remote.private.push",
            "refs/heads/*:refs/heads/*",
        ],
        None,
    )
    .ok();
    run_git(
        &[
            "config",
            "--add",
            "remote.private.push",
            "refs/notes/copycara-map:refs/notes/copycara-map",
        ],
        None,
    )
    .ok();

    println!("[Copycara Init] 2. Creating shadow worktree in .copycara/mirror...");
    if !Path::new(".copycara/mirror").exists() {
        fs::create_dir_all(".copycara")?;
        fs::write(".copycara/.gitignore", "*\n")?;
        // Записываем дефолтный конфиг, если его нет (Фаза 1)
        if !Path::new(".copycara/config.toml").exists() {
            fs::write(
                ".copycara/config.toml",
                CopycaraConfig::default_config_content(),
            )?;
        }
        run_git(&["worktree", "add", "-q", "--detach", ".copycara/mirror"], None)?;
    } else {
        println!("  Worktree already exists. Skipping.");
    }

    println!("[Copycara Init] 3. Installing Copycara hooks...");
    let hooks_dir = Path::new(".git/hooks");
    let exe_path = std::env::current_exe().context("Failed to get current executable path")?;
    let exe_str = exe_path.to_str().unwrap();
    let cfg = CopycaraConfig::load();
    hooks::install_hooks(hooks_dir, exe_str, &cfg)?;

    // Шаг 4: Auto upstream настройка (Фаза 3)
    println!("[Copycara Init] 4. Configuring branch upstream tracking...");
    let current_branch = run_git(&["rev-parse", "--abbrev-ref", "HEAD"], None)?
        .trim()
        .to_string();
    if run_git(&["remote", "get-url", "private"], None).is_ok() {
        run_git(
            &[
                "branch",
                "--set-upstream-to",
                &format!("private/{}", current_branch),
                &current_branch,
            ],
            None,
        )
        .ok();
        println!(
            "  Branch '{}' now tracks 'private/{}'",
            current_branch, current_branch
        );
    } else {
        let current_remote = run_git(
            &["config", &format!("branch.{}.remote", current_branch)],
            None,
        )
        .unwrap_or_default()
        .trim()
        .to_string();
        if current_remote == "origin" {
            run_git(
                &["config", "--unset", &format!("branch.{}.remote", current_branch)],
                None,
            )
            .ok();
            run_git(
                &["config", "--unset", &format!("branch.{}.merge", current_branch)],
                None,
            )
            .ok();
            println!(
                "  Untracked '{}' from 'origin' (хэши различаются из-за DLP-очистки)",
                current_branch
            );
        }
    }

    // Шаг 5: Git config-подсказка для AI-агентов (Фаза 6)
    println!("[Copycara Init] 5. Writing git config hints for AI agents...");
    run_git(&["config", "--local", "copycara.enabled", "true"], None).ok();
    run_git(
        &["config", "--local", "copycara.sync-command", "copycara sync"],
        None,
    )
    .ok();
    run_git(
        &[
            "config",
            "--local",
            "copycara.push-command",
            "copycara push",
        ],
        None,
    )
    .ok();

    println!("\n[Success] Repository initialized with Copycara DLP engine!");
    println!("Hooks point to: {}", exe_str);

    Ok(())
}

fn uninstall_command() -> Result<()> {
    println!("[Copycara Uninstall] Removing DLP engine from repository...");

    if !Path::new(".git").exists() {
        anyhow::bail!("No .git directory found! Please run 'copycara uninstall' inside a git repository.");
    }

    println!("[Copycara Uninstall] 1. Removing Git refspecs routing...");
    run_git(&["config", "--unset-all", "remote.origin.push"], None).ok();
    run_git(&["config", "--unset-all", "remote.private.push"], None).ok();

    println!("[Copycara Uninstall] 2. Removing shadow worktree and .copycara directory...");
    run_git(&["worktree", "remove", "-f", ".copycara/mirror"], None).ok();
    let _ = fs::remove_dir_all(".copycara");

    println!("[Copycara Uninstall] 3. Removing Git hooks...");
    hooks::remove_hooks(Path::new(".git/hooks"));

    println!("\n[Success] Copycara DLP engine has been completely removed from this repository.");
    println!("Standard Git behavior is fully restored.");
    Ok(())
}

// ==========================================
// БЛОК ОЧИСТКИ (TOTAL AST WIPE С ЛОГИРОВАНИЕМ)
// ==========================================

fn apply_dlp_cleanup(dir: &Path) -> Result<()> {
    println!("  [Copycara Engine] Applying uncomment (tree-sitter AST)...");

    let cfg = CopycaraConfig::load();
    let (remove_todo, remove_fixme, remove_doc) = match cfg.cleanup.mode.as_str() {
        "smart" => (false, false, false),
        _ => (true, true, true),
    };
    let ext_map = cfg.cleanup.extension_map.clone();

    let mut processor = Processor::new();
    let config_manager = ConfigManager::from_single_config(dir, Config::default())?;
    let options = ProcessingOptions {
        remove_todo,
        remove_fixme,
        remove_doc,
        custom_preserve_patterns: cfg.cleanup.preserve_patterns.clone(),
        use_default_ignores: false,
        dry_run: false,
        show_diff: false,
        respect_gitignore: false,
        traverse_git_repos: false,
    };

    fn process_single_file(
        path: &Path,
        processor: &mut Processor,
        config_manager: &ConfigManager,
        options: &ProcessingOptions,
        ext_map: &std::collections::HashMap<String, String>,
        ext: &str,
    ) -> Result<()> {
        // Для расширений из extension_map: rename-трюк для передачи в uncomment
        let mapped = ext_map.get(ext).cloned();
        if let Some(ref target_ext) = mapped {
            let mapped_path = path.with_extension(target_ext);
            fs::rename(path, &mapped_path)?;
            let result =
                processor.process_file_with_config(&mapped_path, config_manager, Some(options));
            fs::rename(&mapped_path, path)?;
            match result {
                Ok(r) => {
                    if r.original_content != r.processed_content {
                        fs::write(path, r.processed_content)?;
                        println!("    [DLP] Scrubbed: {:?}", path.file_name().unwrap_or_default());
                    }
                }
                Err(e) => {
                    println!("    [DLP] Skipping {:?}: {}", path.file_name().unwrap_or_default(), e);
                }
            }
        } else {
            match processor.process_file_with_config(path, config_manager, Some(options)) {
                Ok(result) => {
                    if result.original_content != result.processed_content {
                        fs::write(path, result.processed_content)?;
                        println!("    [DLP] Scrubbed: {:?}", path.file_name().unwrap_or_default());
                    }
                }
                Err(e) => {
                    println!(
                        "    [DLP] Skipping {:?}: {}",
                        path.file_name().unwrap_or_default(),
                        e
                    );
                }
            }
        }
        Ok(())
    }

    fn visit_dirs(
        current_dir: &Path,
        processor: &mut Processor,
        config_manager: &ConfigManager,
        options: &ProcessingOptions,
        ext_map: &std::collections::HashMap<String, String>,
    ) -> Result<()> {
        if current_dir.is_dir() {
            for entry in fs::read_dir(current_dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_dir() {
                    if path.file_name().unwrap_or_default() != ".git" {
                        visit_dirs(&path, processor, config_manager, options, ext_map)?;
                    }
                } else if let Some(ext_os) = path.extension() {
                    let ext = ext_os.to_string_lossy().to_lowercase();
                    let valid_exts = [
                        "py", "rs", "js", "ts", "cpp", "c", "h", "hpp", "java", "go", "cs", "rb",
                        "sh",
                    ];

                    if valid_exts.iter().any(|&e| e == ext) || ext_map.contains_key(ext.as_str()) {
                        process_single_file(
                            &path,
                            processor,
                            config_manager,
                            options,
                            ext_map,
                            &ext,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    visit_dirs(dir, &mut processor, &config_manager, &options, &ext_map)?;
    Ok(())
}

// ==========================================
// БЛОК FORWARD SMUDGE (ХУКИ)
// ==========================================

fn process_commit_command(target_hash: &str) -> Result<()> {
    if Path::new(SYNC_STATE_FILE).exists() {
        let original_hash = run_git(&["rev-parse", target_hash], None)?
            .trim()
            .to_string();
        println!("\n[Copycara Engine] Sync state detected. Finalizing reverse patch resolution...");

        let shadow_hash = fs::read_to_string(SYNC_STATE_FILE)?.trim().to_string();

        run_git(
            &[
                "notes",
                "--ref",
                "copycara-map",
                "add",
                "-f",
                "-m",
                &shadow_hash,
                &original_hash,
            ],
            None,
        )?;

        let _ = fs::remove_file(SYNC_STATE_FILE);
        let _ = fs::remove_file(PATCH_FILE);

        println!("[Success] Workspace synced automatically via post-commit hook! Reverse mapping created:");
        println!(
            "Original: {} -> Shadow: {}",
            &original_hash[0..7],
            &shadow_hash[0..7]
        );

        return Ok(());
    }

    let original_hash = run_git(&["rev-parse", target_hash], None)?
        .trim()
        .to_string();
    let original_msg = run_git(&["log", "-1", "--pretty=%B", &original_hash], None)?
        .trim()
        .to_string();

    let mut current_branch = run_git(
        &[
            "branch",
            "--contains",
            &original_hash,
            "--format=%(refname:short)",
        ],
        None,
    )?
    .lines()
    .next()
    .unwrap_or("")
    .trim()
    .to_string();

    if current_branch.is_empty() || current_branch == "HEAD" {
        current_branch = run_git(&["rev-parse", "--abbrev-ref", "HEAD"], None)?
            .trim()
            .to_string();
    }

    println!(
        "\n[Copycara Engine] Processing commit {} on branch '{}'...",
        &original_hash[..7], current_branch
    );

    let original_parent = run_git(&["rev-parse", &format!("{}^", target_hash)], None)
        .unwrap_or_default()
        .trim()
        .to_string();
    let mut shadow_parent = String::new();
    if !original_parent.is_empty() {
        shadow_parent = run_git(
            &["notes", "--ref", "copycara-map", "show", &original_parent],
            None,
        )
        .unwrap_or_default()
        .trim()
        .to_string();
    }

    let mirror_dir = ".copycara/mirror";

    if shadow_parent.is_empty() {
        let _ = run_git(&["checkout", "--orphan", "copycara-main"], Some(mirror_dir));
        let _ = run_git(&["rm", "-rf", "."], Some(mirror_dir));
    } else {
        run_git(&["checkout", "-q", &shadow_parent], Some(mirror_dir))?;
    }

    run_git(
        &["checkout", &original_hash, "--", "."],
        Some(mirror_dir),
    )?;

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

        let shadow_hash =
            run_git_with_stdin(&commit_args, Some(mirror_dir), &original_msg)?
                .trim()
                .to_string();

        run_git(
            &[
                "notes",
                "--ref",
                "copycara-map",
                "add",
                "-f",
                "-m",
                &shadow_hash,
                &original_hash,
            ],
            None,
        )?;
        if current_branch != "HEAD" {
            run_git(
                &[
                    "update-ref",
                    &format!("refs/copycara/heads/{}", current_branch),
                    &shadow_hash,
                ],
                None,
            )?;
        }
        println!("[Copycara Engine] Shadow commit created: {}.", &shadow_hash[..7]);
    } else {
        println!("[Copycara Engine] Diff is empty. Dropping commit from shadow history.");
        run_git(
            &[
                "notes",
                "--ref",
                "copycara-map",
                "add",
                "-f",
                "-m",
                &shadow_parent,
                &original_hash,
            ],
            None,
        )?;
        if current_branch != "HEAD" {
            run_git(
                &[
                    "update-ref",
                    &format!("refs/copycara/heads/{}", current_branch),
                    &shadow_parent,
                ],
                None,
            )?;
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
    println!(
        "[Copycara Sync] Resuming sync process for shadow commit {}...",
        &new_shadow_hash[..7]
    );

    let unmerged = run_git(&["ls-files", "-u"], None)?;
    if !unmerged.trim().is_empty() {
        anyhow::bail!("You still have unresolved conflicts. Please fix them, run 'git add', and try again.");
    }

    println!("[Copycara Sync] Committing resolved changes and updating notes map...");
    let commit_msg = run_git(&["log", "-1", "--pretty=%B", &new_shadow_hash], Some(".copycara/mirror"))?
        .trim()
        .to_string();

    let commit_res = run_git(
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "-q",
            "-m",
            &format!("Merge remote sync:\n\n{}", commit_msg),
        ],
        None,
    );
    if let Err(e) = commit_res {
        anyhow::bail!(
            "Failed to commit. Did you run 'git add' on the resolved files?\nError: {:?}",
            e
        );
    }

    let new_workspace_hash = run_git(&["rev-parse", "HEAD"], None)?
        .trim()
        .to_string();
    run_git(
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
    )?;

    let _ = fs::remove_file(SYNC_STATE_FILE);
    let _ = fs::remove_file(PATCH_FILE);

    println!("\n[Success] Workspace synced! Reverse mapping created:");
    println!(
        "Original: {} -> Shadow: {}",
        &new_workspace_hash[0..7],
        &new_shadow_hash[0..7]
    );

    Ok(())
}

fn sync_start() -> Result<()> {
    if Path::new(SYNC_STATE_FILE).exists() {
        anyhow::bail!("A sync is already in progress! Please resolve conflicts and run 'git commit'.");
    }

    println!("[Copycara Sync] Starting reverse patching process...");

    let current_branch = run_git(&["branch", "--show-current"], None)?
        .trim()
        .to_string();
    if current_branch.is_empty() {
        anyhow::bail!("Not on any branch. Please checkout a branch first.");
    }
    println!("[Copycara Sync] Current branch: {}", current_branch);

    println!("[Copycara Sync] 1. Fetching clean code from origin...");
    run_git(&["fetch", "origin", &current_branch], None)?;

    println!("[Copycara Sync] 2. Syncing shadow graph in .copycara/mirror...");
    let old_shadow_hash = run_git(&["rev-parse", &format!("refs/copycara/heads/{}", current_branch)], None)?
        .trim()
        .to_string();

    run_git(&["checkout", "-q", &old_shadow_hash], Some(".copycara/mirror"))?;

    let merge_res = run_git(
        &["merge", "--no-edit", &format!("origin/{}", current_branch)],
        Some(".copycara/mirror"),
    );
    if merge_res.is_err() {
        run_git(&["merge", "--abort"], Some(".copycara/mirror")).ok();
        anyhow::bail!("Merge conflict in shadow graph! Aborting.");
    }

    let new_shadow_hash = run_git(&["rev-parse", "HEAD"], Some(".copycara/mirror"))?
        .trim()
        .to_string();
    run_git(
        &[
            "update-ref",
            &format!("refs/copycara/heads/{}", current_branch),
            &new_shadow_hash,
        ],
        None,
    )?;

    println!("[Copycara Sync] 3. Extracting clean patch...");
    let patch_content = run_git(
        &["diff", &old_shadow_hash, &new_shadow_hash],
        Some(".copycara/mirror"),
    )?;

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
        println!("After that, simply run 'git commit -m \"Resolve sync conflict\"' to finalize the sync automatically.");
        return Err(e);
    }

    println!("[Copycara Sync] 5. Committing changes and updating notes map...");
    let commit_msg = run_git(&["log", "-1", "--pretty=%B", &new_shadow_hash], Some(".copycara/mirror"))?
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
            &format!("Merge remote sync:\n\n{}", commit_msg),
        ],
        None,
    )?;

    let new_workspace_hash = run_git(&["rev-parse", "HEAD"], None)?
        .trim()
        .to_string();
    run_git(
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
    )?;

    let _ = fs::remove_file(PATCH_FILE);

    println!("\n[Success] Workspace synced! Reverse mapping created:");
    println!(
        "Original: {} -> Shadow: {}",
        &new_workspace_hash[0..7],
        &new_shadow_hash[0..7]
    );

    Ok(())
}

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Commands::Init => init_command(),
        Commands::Uninstall => uninstall_command(),
        Commands::ProcessCommit { target_hash } => process_commit_command(target_hash),
        Commands::Sync { resume } => {
            if *resume {
                sync_continue()
            } else {
                sync_start()
            }
        }
        Commands::Push { force, no_private } => push::push_command(*force, *no_private),
    };

    if let Err(e) = result {
        eprintln!("Error: {:?}", e);
        std::process::exit(1);
    }
}

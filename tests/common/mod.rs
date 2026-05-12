// Shared test utilities for integration tests.
//
// Provides TestEnv for creating temporary git repositories,
// running the copycara binary, and cleaning up.

use std::path::{Path, PathBuf};
use std::process::Command;

pub fn copycara_bin() -> PathBuf {
    let mut path = std::env::current_exe().expect("current exe path");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    let bin = path.join("copycara");
    if !bin.exists() {
        panic!("copycara binary not found at {:?}. Build with `cargo build` first.", bin);
    }
    bin
}

pub struct TestEnv {
    pub root: PathBuf,
    pub public: PathBuf,
    pub private: PathBuf,
    pub workspace: PathBuf,
}

impl TestEnv {
    pub fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "copycara-test-{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let public = root.join("public.git");
        let private = root.join("private.git");
        let workspace = root.join("workspace");

        std::fs::create_dir_all(&root).expect("create test root");

        run_git(&["init", "--bare", "public.git"], Some(&root));
        run_git(&["init", "--bare", "private.git"], Some(&root));
        run_git(&["clone", public.to_str().unwrap(), "workspace"], Some(&root));
        run_git(&["remote", "add", "private", private.to_str().unwrap()], Some(&workspace));
        run_git(&["config", "user.email", "test@copycara"], Some(&workspace));
        run_git(&["config", "user.name", "Copycara Test"], Some(&workspace));

        Self { root, public, private, workspace }
    }

    pub fn copycara(&self, args: &[&str]) -> CommandResult {
        let bin = copycara_bin();
        let output = Command::new(&bin)
            .args(args)
            .current_dir(&self.workspace)
            .output()
            .expect("copycara execution failed");
        CommandResult {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }

    pub fn git(&self, args: &[&str]) -> CommandResult {
        run_git_cmd(args, Some(&self.workspace))
    }

    pub fn git_bare(&self, dir: &Path, args: &[&str]) -> CommandResult {
        let mut full_args = vec!["--git-dir", dir.to_str().unwrap()];
        full_args.extend_from_slice(args);
        run_git_cmd(&full_args, None)
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub struct CommandResult {
    pub status: std::process::ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

impl CommandResult {
    pub fn success(&self) -> bool {
        self.status.success()
    }

    pub fn stdout_contains(&self, s: &str) -> bool {
        self.stdout.contains(s)
    }

    pub fn stderr_contains(&self, s: &str) -> bool {
        self.stderr.contains(s)
    }
}

fn run_git(args: &[&str], dir: Option<&Path>) {
    let result = run_git_cmd(args, dir);
    assert!(result.success(), "git {} failed:\n{}", args.join(" "), result.stderr);
}

fn run_git_cmd(args: &[&str], dir: Option<&Path>) -> CommandResult {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    let output = cmd.output().expect("git execution failed");
    CommandResult {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

//! Git command helpers for Copycara DLP Engine.
//!
//! Provides wrappers around `git` CLI: execute commands, pipe stdin, set executable permissions.

use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::str;

pub fn run_git(args: &[&str], dir: Option<&str>) -> Result<String> {
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

pub fn run_git_with_stdin(args: &[&str], dir: Option<&str>, stdin_data: &str) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    cmd.env_remove("GIT_DIR").env_remove("GIT_WORK_TREE").env_remove("GIT_INDEX_FILE");

    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());

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

/// Make a file executable (Unix: chmod +x; Windows: no-op).
#[cfg(unix)]
pub fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(windows)]
pub fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

pub fn write_executable_script(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content)?;
    set_executable(path)?;
    Ok(())
}

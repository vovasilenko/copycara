// CLI integration tests — verify the binary can be invoked and shows help.

mod common;

#[test]
fn test_help_shows_commands() {
    let result = run_copycara(&["--help"]);
    assert!(result.success());
    assert!(result.stdout.contains("init"));
    assert!(result.stdout.contains("uninstall"));
    assert!(result.stdout.contains("push"));
    assert!(result.stdout.contains("sync"));
    assert!(result.stdout.contains("Topological Git DLP Engine"));
}

#[test]
fn test_init_help() {
    let result = run_copycara(&["init", "--help"]);
    assert!(result.success());
    assert!(result.stdout.contains("Initialise shadow mirror"));
}

#[test]
fn test_push_help() {
    let result = run_copycara(&["push", "--help"]);
    assert!(result.success());
    assert!(result.stdout.contains("--force"));
    assert!(result.stdout.contains("--no-private"));
}

#[test]
fn test_sync_help() {
    let result = run_copycara(&["sync", "--help"]);
    assert!(result.success());
    assert!(result.stdout.contains("--continue"));
}

#[test]
fn test_version() {
    let result = run_copycara(&["--version"]);
    assert!(result.success());
    assert!(result.stdout.contains("0.2"));
}

fn run_copycara(args: &[&str]) -> common::CommandResult {
    let bin = common::copycara_bin();
    let output =
        std::process::Command::new(&bin).args(args).output().expect("copycara execution failed");
    common::CommandResult {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

// End-to-end integration tests.
//
// Creates a temporary git environment (public.git, private.git, workspace),
// runs copycara init, makes commits, and verifies DLP and push work.

mod common;

#[test]
fn test_init_autofix() {
    let env = common::TestEnv::new();
    let result = env.copycara(&["init"]);
    assert!(result.success(), "init failed:\n{}", result.stderr);
    // Autofix should create an initial commit
    assert!(result.stdout.contains("initial commit"), "autofix not triggered:\n{}", result.stdout);
    assert!(result.stdout.contains("[Success]"), "init not successful:\n{}", result.stdout);
    // Config file should exist
    assert!(env.workspace.join(".copycara/config.toml").exists());
    // Hooks should exist
    assert!(env.workspace.join(".git/hooks/post-commit").exists());
    assert!(env.workspace.join(".git/hooks/pre-push").exists());
    assert!(env.workspace.join(".git/hooks/post-checkout").exists());
}

#[test]
fn test_dlp_scrubs_comments() {
    let env = common::TestEnv::new();
    let _ = env.copycara(&["init"]);

    // Create a .py file with comments and commit it
    let main_py = env.workspace.join("main.py");
    std::fs::write(&main_py, "print('hello')\n# TODO: this should be removed\n# DLP-DROP: secret\n").unwrap();
    env.git(&["add", "main.py"]);
    let commit = env.git(&["commit", "-m", "feat: add main.py"]);
    assert!(commit.success());

    // push clean code to origin
    let push = env.copycara(&["push"]);
    assert!(push.success(), "push failed:\n{}", push.stderr);

    // Verify public origin has clean code (no comments)
    let public_main = env.git_bare(&env.public, &["show", "HEAD:main.py"]);
    assert!(public_main.success());
    assert!(!public_main.stdout.contains("TODO"), "TODO leaked to public:\n{}", public_main.stdout);
    assert!(!public_main.stdout.contains("DLP-DROP"), "DLP-DROP leaked to public:\n{}", public_main.stdout);
    assert!(public_main.stdout.contains("print('hello')"), "functional code missing:\n{}", public_main.stdout);
}

#[test]
fn test_init_on_existing_commit_is_idempotent() {
    let env = common::TestEnv::new();
    // Create an initial commit first, then init
    env.git(&["commit", "--allow-empty", "-m", "existing init"]);
    let result = env.copycara(&["init"]);
    assert!(result.success(), "init on existing commit failed:\n{}", result.stderr);
    assert!(!result.stdout.contains("initial commit"), "autofix should not trigger when HEAD exists");
}

#[test]
fn test_push_to_private_backup() {
    let env = common::TestEnv::new();
    let _ = env.copycara(&["init"]);

    // Create and commit a file
    let main_py = env.workspace.join("main.py");
    std::fs::write(&main_py, "# private comment\nx = 1\n").unwrap();
    env.git(&["add", "main.py"]);
    let _ = env.git(&["commit", "-m", "feat: init"]);

    // Push with copycara
    let push = env.copycara(&["push"]);
    assert!(push.success(), "push failed:\n{}", push.stderr);

    // Private backup should have the comment
    let private_main = env.git_bare(&env.private, &["show", "HEAD:main.py"]);
    assert!(private_main.success());
    assert!(private_main.stdout.contains("private comment"), "private backup missing comment:\n{}", private_main.stdout);
}

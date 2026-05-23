# Changelog

## [0.2.1] - 2026-05-23

### Fixed
- **Config dead code eliminated**: `extra_extensions`, `auto_push_private`, and `force_with_lease`
  are now fully operational in DLP and push modules (were deserialized but ignored)
- **Uninstall now removes git config hints** (`copycara.enabled/sync-command/push-command`)
- **Test race condition fixed**: atomic counter replaces `SystemTime::as_nanos()` for test directory names

### Added
- **90+ file extensions** now supported out of the box (was 14) — all languages from `uncomment` v3.0.2
- **CI now runs `test.sh`** (full E2E sandbox) in addition to `cargo test`
- `test.sh` sandbox path parametrized via `COPYCARA_SANDBOX` env var for CI compatibility

### Changed
- README language claims updated from "all languages / dozens" to specific count + extension mechanisms
- Cargo.toml: fixed repository URL to `https://github.com/vovasilenko/copycara`

## [0.2.0] - 2026-05-12

### Added
- Pre-push hook: blocks `git push origin <branch>` with structured error message,
  protecting AI agents from accidentally leaking annotated code to public repos
- `copycara push` command: safely pushes clean code to origin
  and dirty backup to private; `--force` uses `--force-with-lease`
- Configuration via `.copycara/config.toml` with cleanup mode (all/smart),
  extension mapping, preserve patterns, hook toggles, and push settings
- Extension rename-trick: unknown extensions (e.g. `.cu` → C++) are
  temporarily renamed to a known extension before tree-sitter processing
- Autofix: `copycara init` creates an empty commit if the repo has no HEAD
- Auto upstream: init redirects branch tracking to `private` (or removes
  origin tracking) to prevent `diverged` in `git status`
- Post-checkout hook: new branches get auto-configured upstream
- Git config hints: `copycara.enabled`, `sync-command`, `push-command` for AI agents
- Rustfmt, clippy, justfile, CHANGELOG, unit tests for config and hooks

### Changed
- Renamed project from `copycara-mcp` to `copycara`
- Replaced custom FSM lexer (`strip_comments`) with `uncomment` (tree-sitter AST)
- Split monolithic `main.rs` (808 lines) into modular structure:
  `cli.rs`, `git.rs`, `config.rs`, `hooks.rs`, `init.rs`, `dlp.rs`,
  `commit.rs`, `sync.rs`, `push.rs`
- Release profile: LTO, single codegen unit, abort on panic, symbol stripping
- Rust edition 2024, pedantic clippy linting set to warn

### Fixed
- `git status` no longer shows `diverged` after `copycara init`
- Empty repository init no longer fails
- `copycara push` no longer blocked by its own pre-push hook
- File permissions set correctly on Unix via `set_executable`

## [0.1.0] - 2026-04-01

### Added
- DLP engine with FSM-based comment removal
- Forward smudge (post-commit / post-merge hooks)
- Reverse smudge (`copycara sync`)
- Git notes mapping (`refs/notes/copycara-map`)
- Shadow worktree in `.copycara/mirror`
- `copycara init` and `copycara uninstall` commands
- Single-file implementation in `main.rs`

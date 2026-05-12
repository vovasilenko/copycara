# Contributing to Copycara

Thank you for your interest in contributing!

## Getting started

```bash
git clone <your-fork>
cd copycara
cargo build
bash test.sh
```

## Before submitting a PR

### 1. Check formatting

```bash
cargo fmt --check
```

### 2. Check clippy

```bash
cargo clippy -- -D warnings
```

### 3. Run all tests

```bash
cargo test          # unit + integration tests (21+)
bash test.sh        # full E2E sandbox tests (42)
```

### 4. Conventional commits

Commit messages must follow the [Conventional Commits](https://www.conventionalcommits.org/) specification:

```
feat: add --dry-run flag
fix: resolve diverged warning during init
docs: update README with sync examples
refactor: extract hooks.rs from main.rs
test: add DLP scrubbing integration test
```

Allowed types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `perf`, `ci`.

## Code style

- `max_width = 100`, tabs = 4 spaces (enforced by `rustfmt.toml`)
- No `unsafe` code (`#![forbid(unsafe_code)]`)
- All modules must have `//!` doc comments
- New public functions must have `///` doc comments
- `anyhow::Result` for fallible functions (this is a binary crate)

## Testing

- **Unit tests**: add `#[cfg(test)] mod tests { }` next to the code in `src/*.rs`
- **Integration tests**: add scenarios to `tests/` that invoke the binary via `std::process::Command`
- **E2E tests**: update `test.sh` if the full sandbox workflow changes

## License

This project is dual-licensed under MIT and Apache-2.0. By contributing you agree to license your work under these terms.

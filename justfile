build:
    cargo build --release

test:
    cargo test
    bash test.sh

lint:
    cargo fmt --check
    cargo clippy -- -D warnings

fix:
    cargo fmt
    cargo clippy --fix --allow-dirty

install:
    cargo install --path . --locked

clean:
    cargo clean
    rm -rf {{home}}/Lab/copycara-sandbox

audit:
    cargo deny check 2>/dev/null || echo "  [skip] Install cargo-deny: cargo install cargo-deny"
    cargo audit 2>/dev/null || echo "  [skip] Install cargo-audit: cargo install cargo-audit"

check: lint test build
    @echo "All checks passed"

ci: lint test build
    @echo "CI checks passed"

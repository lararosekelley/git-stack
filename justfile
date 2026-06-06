# Development
# -----------

# Install dependencies
install: install-rust install-js

install-rust:
    cargo fetch

install-js:
    npm install

# Build release binary
build:
    cargo build --release

clean:
    cargo clean

# Check Cargo package contents
package:
    cargo package --no-verify

# Run publish dry-run checks
publish-dry-run:
    cargo publish --dry-run

# Plan release artifacts
dist-plan:
    dist plan

# Bump the crate version (patch|minor|major); commit and tag manually afterward
bump part:
    cargo run --features dev-tools --bin git-stk-bump -- {{part}}

# Generate shell completions and man pages
generate-assets:
    cargo run --features generate --bin git-stk-generate -- all target/generated

# Generate shell completions
generate-completions:
    cargo run --features generate --bin git-stk-generate -- completions target/generated/completions

# Generate man pages
generate-man:
    cargo run --features generate --bin git-stk-generate -- man target/generated/man

# Testing
# -------

# Run all tests
test:
    cargo test

# Code formatting
# ---------------

# Format and lint all
check: format lint test

# Format Rust code
format:
    cargo fmt

# Linting
# -------

# Lint with clippy and check formatting
lint: lint-rust lint-md

lint-rust:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings

lint-md:
    npx markdownlint-cli2 "**/*.md"

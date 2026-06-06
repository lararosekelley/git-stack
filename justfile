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

# Generate shell completions and man pages
generate-assets:
    cargo run --features generate --bin git-stack-generate -- all target/generated

# Generate shell completions
generate-completions:
    cargo run --features generate --bin git-stack-generate -- completions target/generated/completions

# Generate man pages
generate-man:
    cargo run --features generate --bin git-stack-generate -- man target/generated/man

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

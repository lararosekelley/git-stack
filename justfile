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

set shell := ["bash", "-euo", "pipefail", "-c"]

# List all available recipes
default:
    @just --list

# Install dependencies and git hooks
setup:
    pnpm install
    pnpm exec lefthook install

# Run Tauri app in development mode
dev:
    pnpm tauri dev

# Run all linters, formatters (check), and typechecks
check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    pnpm run lint
    pnpm run check

# Format both Rust and frontend code
fmt:
    cargo fmt --all
    pnpm run fmt

# Run all Rust and frontend unit tests
test:
    cargo test --workspace
    pnpm run test

# Run Criterion performance benchmarks
bench:
    cargo bench --workspace

# Build standalone fossil CLI/MCP binary in release mode
build-cli:
    cargo build --release -p fossil-cli

# Build production OS binary/installer for current platform
build:
    pnpm tauri build

# Kansha task runner. `just` lists recipes.

set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

default:
    @just --list

# Run the full test suite (TEST-010, TEST-120)
test: test-rust test-frontend

# Rust tests only (engine, scenarios, properties)
test-rust:
    cargo test --workspace

# Frontend tests only (Vitest)
test-frontend:
    npm test

# Formatting, lints, type-checking, and tests: what CI runs
check: fmt-check clippy typecheck test

# Format all Rust code
fmt:
    cargo fmt --all

# Fail if Rust code is not formatted
fmt-check:
    cargo fmt --all -- --check

# Lint; warnings are errors
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# svelte-check: type errors in .svelte and .ts files
typecheck:
    npm run check

# Run the desktop app in dev mode (hot reload; regenerates TS bindings)
dev:
    npm run tauri dev

# Production build (frontend + installers for the current platform)
build:
    npm run tauri build

# Regenerate src/lib/types/bindings.ts from the Rust command signatures
bindings:
    cargo build --manifest-path src-tauri/Cargo.toml

# Run scenarios from one file or directory, e.g. `just scenario tests/scenarios/harness`
scenario $KANSHA_SCENARIOS:
    cargo test -p kansha-core --test scenarios -- scenarios --exact --nocapture

# Show how failing scenarios are reported (this recipe is expected to fail)
scenario-demo-fail:
    just scenario crates/kansha-core/tests/fixtures/failing

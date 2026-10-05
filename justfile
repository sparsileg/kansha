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

# Large-book timings: 100,000+ transactions, 12 accounts (NFR-040, NFR-050)
perf:
    cargo test --release -p kansha-core --test integration perf:: -- --ignored --nocapture

# Engine requirements no test cites (TEST-070)
trace:
    cargo test -p kansha-core --test trace -- --ignored --nocapture

# Engine test coverage (TEST-150): summary here, detail in target/llvm-cov/html/index.html
cov:
    cargo llvm-cov -p kansha-core --html
    cargo llvm-cov report -p kansha-core --summary-only

# Review changed report snapshots (TEST-090): accept or reject each diff
snap-review:
    cargo insta review

# Trace, then coverage; a trace failure does not stop coverage
report:
    -just trace
    just cov

# Every automated release check, with a PASS/FAIL/NOTE summary; BASE = last release (default: latest tag)
[unix]
release-check BASE="":
    scripts/release-check.sh {{BASE}}

[windows]
release-check BASE="":
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/release-check.ps1 {{BASE}}

# Set the app version in all the places it is written, e.g. `just version 0.9.1`
[unix]
version V:
    scripts/set-version.sh {{V}}

[windows]
version V:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/set-version.ps1 {{V}}

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

# Release exe only, no installers (target/release/)
build-exe:
    npm run tauri build -- --no-bundle

# Regenerate src/lib/types/bindings.ts from the Rust command signatures
bindings:
    cargo test -p kansha write_bindings -- --ignored

# Run scenarios from one file or directory, e.g. `just scenario tests/scenarios/harness`
scenario $KANSHA_SCENARIOS:
    cargo test -p kansha-core --test scenarios -- scenarios --exact --nocapture

# Show how failing scenarios are reported (this recipe is expected to fail)
scenario-demo-fail:
    just scenario crates/kansha-core/tests/fixtures/failing

# Touches every workspace .rs file and rebuilds (build, test, clippy), so
# the workspace crates recompile once; any incremental dir those builds
# don't write is dead.
#
# Delete stale incremental dirs from target/debug (full workspace rebuild)
[unix]
sweep:
    #!/usr/bin/env bash
    set -euo pipefail
    marker=$(mktemp target/sweep-marker.XXXXXX)
    trap 'rm -f "$marker"' EXIT
    sleep 1
    find crates src-tauri -name target -prune -o -name '*.rs' -exec touch -c {} +
    cargo build --workspace
    cargo test --workspace --no-run
    cargo test -p kansha --no-run
    cargo clippy --workspace --all-targets -- -D warnings
    before=$(du -sh target | cut -f1)
    n=0
    for d in target/debug/incremental/*/; do
        if [ -z "$(find "$d" -newer "$marker" -print -quit)" ]; then
            rm -rf -- "$d"
            n=$((n + 1))
        fi
    done
    echo "sweep: removed $n incremental dirs; target $before -> $(du -sh target | cut -f1)"

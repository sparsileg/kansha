# Kansha task runner. `just` lists recipes.

set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

default:
    @just --list

# Run the full test suite (TEST-010)
test:
    cargo test --workspace

# Formatting, lints, and tests: what CI runs
check: fmt-check clippy test

# Format all Rust code
fmt:
    cargo fmt --all

# Fail if Rust code is not formatted
fmt-check:
    cargo fmt --all -- --check

# Lint; warnings are errors
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Run scenarios from one file or directory, e.g. `just scenario tests/scenarios/harness`
scenario $KANSHA_SCENARIOS:
    cargo test -p kansha-core --test scenarios -- scenarios --exact --nocapture

# Show how failing scenarios are reported (this recipe is expected to fail)
scenario-demo-fail:
    just scenario crates/kansha-core/tests/fixtures/failing

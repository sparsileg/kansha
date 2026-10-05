#!/usr/bin/env bash
# Release checks (devdocs/release-checklist.md §3, §4). Runs every
# check, then prints one line per check: PASS, FAIL, or NOTE (look at
# it; not a failure by itself). Exit status is 1 if any check failed.
#
# Usage: scripts/release-check.sh [BASE]
#   BASE: the commit or tag of the last release. Defaults to the latest
#   tag; with no tag, the checks that compare with it are skipped.
set -uo pipefail
cd "$(dirname "$0")/.."

log=target/release-check
mkdir -p "$log"
results=()
failed=0
pass() { results+=("PASS  $1"); }
fail() { results+=("FAIL  $1"); failed=1; }
note() { results+=("NOTE  $1"); }
step() { printf '\n=== %s\n' "$1"; }

base=${1:-$(git describe --tags --abbrev=0 2>/dev/null || true)}

# 1. Format, lints, type-checking, every test.
step "just check"
if just check 2>&1 | tee "$log/check.txt"; then
    pass "just check"
else
    fail "just check (see $log/check.txt)"
fi

# 2. The version is the same in every place it is written.
step "version"
v_cargo=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
v_pkg=$(node -p 'require("./package.json").version')
v_lock=$(node -p 'const l=require("./package-lock.json"); l.version+" "+l.packages[""].version')
v_tauri=$(node -p 'require("./src-tauri/tauri.conf.json").version')
v_test=$(sed -n 's/.*toBe("\(.*\)").*/\1/p' src/lib/version.test.ts)
v_clock=$(grep -A1 -E '^name = "kansha(-core)?"$' Cargo.lock | sed -n 's/^version = "\(.*\)"/\1/p' | tr '\n' ' ')
all="$v_cargo $v_pkg $v_lock $v_tauri $v_test $v_clock"
if [ -z "$(echo $all | tr ' ' '\n' | grep -vx "$v_cargo")" ]; then
    pass "version $v_cargo in all places"
else
    fail "versions differ: Cargo.toml $v_cargo, package.json $v_pkg, package-lock $v_lock, tauri.conf.json $v_tauri, version.test.ts $v_test, Cargo.lock $v_clock (fix with: just version X.Y.Z)"
fi

# 3. Bindings match the command signatures.
step "bindings"
f=src/lib/types/bindings.ts
before=$(git hash-object "$f")
if just bindings >"$log/bindings.txt" 2>&1; then
    if [ "$before" = "$(git hash-object "$f")" ]; then
        pass "bindings unchanged"
    else
        fail "just bindings changed $f; review and commit the diff"
    fi
else
    fail "just bindings (see $log/bindings.txt)"
fi

# 4. Snapshots: none pending; list those changed since the last release.
step "snapshots"
pending=$(find crates -name '*.snap.new' | tr '\n' ' ')
if [ -n "$pending" ]; then
    fail "pending snapshots: $pending(run: just snap-review)"
else
    pass "no pending snapshots"
fi
if [ -n "$base" ]; then
    changed=$(git diff --name-only "$base" -- '*.snap' | tr '\n' ' ')
    [ -n "$changed" ] && note "snapshots changed since $base; name each in the release notes: $changed"
fi

# 5. Released migrations are never edited.
step "migrations"
if [ -n "$base" ]; then
    edited=$(git diff --name-status "$base" -- crates/kansha-core/src/persistence/migrations | grep -v '^A' | tr '\n' ' ')
    if [ -n "$edited" ]; then
        fail "released migrations changed since $base: $edited"
    else
        pass "migrations: only new files since $base"
    fi
    added=$(git diff --name-only --diff-filter=A "$base" -- crates/kansha-core/src/persistence/migrations | tr '\n' ' ')
    [ -n "$added" ] && note "new migrations (Schema change; each needs a migration test): $added"
else
    note "migrations not compared: no base (pass the last release's commit)"
fi

# 6. Traceability: the only uncited IDs are those spec §23 says are not built.
step "just trace"
just trace >"$log/trace.txt" 2>&1
uncited=$(sed -n 's/^ *not cited: //p' "$log/trace.txt" | sort)
s23=$(sed -n '/^### 23\./,/^### 24\./p' devdocs/kansha-spec.md)
unexpected=""
for id in $uncited; do
    grep -q "$id" <<<"$s23" || unexpected+="$id "
done
summary=$(grep -m1 'engine requirements cited' "$log/trace.txt")
if [ -z "$summary" ]; then
    fail "just trace did not run (see $log/trace.txt)"
elif [ -n "$unexpected" ]; then
    fail "requirements not cited by a test and not listed in spec §23: $unexpected"
else
    pass "trace: $summary; uncited are all in §23: $(echo $uncited)"
fi

# 7. Coverage: record it beside the last release's.
step "just cov"
if just cov >"$log/cov.txt" 2>&1; then
    total=$(grep '^TOTAL' "$log/cov.txt" | awk '{print "regions " $4 ", functions " $7 ", lines " $10}')
    note "coverage $total; record it in the release notes (detail: target/llvm-cov/html/index.html)"
else
    fail "just cov (see $log/cov.txt)"
fi

# 8. Large-book timings.
step "just perf"
if just perf 2>&1 | tee "$log/perf.txt"; then
    note "timings in $log/perf.txt; record them in the release notes"
else
    fail "just perf: a timing is over its limit (see $log/perf.txt)"
fi

# 9. Builds on the minimum supported Rust version.
step "MSRV"
if rustup toolchain list | grep -q '^1\.93'; then
    if cargo +1.93 check --workspace >"$log/msrv.txt" 2>&1; then
        pass "builds with Rust 1.93"
    else
        fail "cargo +1.93 check (see $log/msrv.txt)"
    fi
else
    note "MSRV not checked: run once: rustup toolchain install 1.93"
fi

# 10. Everything is committed.
step "git status"
if [ -z "$(git status --porcelain)" ]; then
    pass "working tree clean"
else
    note "uncommitted changes; commit them before tagging"
fi

printf '\n=== Release check%s\n' "${base:+ against $base}"
printf '%s\n' "${results[@]}" | tee "$log/summary.txt"
exit $failed

#!/usr/bin/env bash
# Sets the app version everywhere it is written: Cargo.toml,
# package.json, package-lock.json, src-tauri/tauri.conf.json,
# src/lib/version.test.ts, and Cargo.lock.
#
# Usage: scripts/set-version.sh X.Y.Z
set -euo pipefail
cd "$(dirname "$0")/.."

v=${1:?usage: set-version.sh X.Y.Z}
[[ $v =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "not X.Y.Z: $v" >&2; exit 1; }

sed -i "0,/^version = \".*\"/s//version = \"$v\"/" Cargo.toml
npm version "$v" --no-git-tag-version --allow-same-version >/dev/null
sed -i "0,/\"version\": \".*\"/s//\"version\": \"$v\"/" src-tauri/tauri.conf.json
sed -i -e "s/it(\"is [0-9.]*\"/it(\"is $v\"/" -e "s/toBe(\"[0-9.]*\")/toBe(\"$v\")/" src/lib/version.test.ts
cargo update --workspace --offline >/dev/null 2>&1 || cargo update --workspace >/dev/null
echo "version set to $v:"
git diff --stat -- Cargo.toml Cargo.lock package.json package-lock.json src-tauri/tauri.conf.json src/lib/version.test.ts

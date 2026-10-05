# Kansha — Release Checklist

Work through this list before a build is installed over the
production book or tagged as a version (spec §21). Copy it into the
release notes and tick each step. A step that is skipped says why.

Release: X.Y.Z · last release: A.B.C · date: ______

No release is tagged yet. Until one is, use the commit of the build
installed on the book as the last release (BASE below).

Commands run from the repository folder. `release-check` and
`version` are bash scripts on Kubuntu and PowerShell scripts
(`scripts/*.ps1`, Windows PowerShell 5.1 or later) on Windows; `just`
picks the right one.

## 1. Scope

1. [ ] Every requirement meant for this release is built. The rest
   are listed as not built in spec §23.
2. [ ] The spec version is bumped, and Appendix A has an entry for
   each change.
3. [ ] Phase notes are written for any phase that ended.
4. [ ] Open decisions this release depends on are decided (spec
   Part V).

## 2. Version number

1. [ ] Set the new version everywhere it is written:

   ```sh
   just version X.Y.Z
   ```

   This sets `Cargo.toml`, `package.json`, `package-lock.json`,
   `src-tauri/tauri.conf.json`, `src/lib/version.test.ts`, and
   `Cargo.lock`, then lists the changed files.

## 3. Automated checks

1. [ ] Run every automated check:

   ```sh
   just release-check BASE
   ```

   BASE is the last release's tag or commit. Leave it out once tags
   exist; the latest tag is used. It takes several minutes, since it
   runs the tests three times (plain, coverage, and the release
   build for timings).

2. [ ] Every line of the summary at the end is PASS or NOTE. The
   summary is also saved in `target/release-check/summary.txt`, with
   each check's full output beside it.

   | Check | PASS means | On FAIL |
   |---|---|---|
   | just check | Format, clippy, svelte-check, and every Rust and frontend test pass | Fix, rerun |
   | version | All the places in §2 agree | `just version X.Y.Z` |
   | bindings | `bindings.ts` matches the Rust command signatures | Review the diff `just bindings` made; commit it |
   | snapshots | No pending report snapshots (TEST-090) | `just snap-review` |
   | migrations | No released migration was edited | Undo the edit; add a new migration instead |
   | trace | Every uncited requirement is one §23 lists as not built (TEST-070) | Add a test, or list it in §23 |
   | MSRV | The workspace builds with Rust 1.85 | Replace the newer language feature |
   | perf | The large-book timings are within limits (NFR-040, NFR-050) | Known issue in the release notes, or fix |

3. [ ] Act on each NOTE:

   - **coverage**: record the line coverage in the release notes
     beside the last release's (TEST-150). If a money, lot, or report
     file dropped, look at its unrun lines in
     `target/llvm-cov/html/index.html`.
   - **timings**: record them in the release notes.
   - **snapshots changed**: each changed report layout was meant; name
     it in the release notes.
   - **new migrations**: a ⚠ Schema change. Each has a migration test
     (TEST-100), and §4 below matters more than usual.
   - **MSRV not checked**: install the toolchain once, then rerun:

     ```sh
     rustup toolchain install 1.85
     ```

   - **uncommitted changes**: expected before the release commit.

4. [ ] Optional: no security advisory touches Kansha.

   ```sh
   cargo install cargo-audit   # once
   cargo audit
   npm audit --omit=dev
   ```

5. [ ] Pinned dependencies (specta, age, zip, …) are unchanged, or the
   change was agreed:

   ```sh
   git diff BASE -- Cargo.toml crates/*/Cargo.toml src-tauri/Cargo.toml package.json
   ```

6. [ ] Windows release only: on Windows, run the checks by hand
   (NFR-010):

   ```powershell
   just check
   just report
   ```

7. [ ] While CI is enabled (TEST-140): the latest run is green on
   Ubuntu and Windows.

To run only trace and coverage while working:

```sh
just report
```

## 4. Data safety

The app migrates the book when it opens it, and an older build then
refuses that book (spec §21). There is no way back except a backup.

1. [ ] In the current build, make a backup and verify it:

   1. File > Back Up Now.
   2. Settings > Verify backup…, and pick that backup.
   3. In Settings, check the backup folder is where you expect.

2. [ ] In the current build, export the figures to compare with,
   as CSV:

   1. Net Worth for today.
   2. Tax Schedule for last year.

3. [ ] Trial the new build on a copy of the book:

   1. Copy the book and its key file to a scratch folder.
   2. Run the new build on the copy (from the repository, or the
      installed binary with the same variable):

      ```sh
      KANSHA_DB=/path/to/copy/kansha.db just dev
      ```

   3. The copy opens and File > Integrity Check is clean.
   4. Net Worth and the Tax Schedule match the CSVs, except where
      this release meant them to change.

4. [ ] Keep the last release's installer until the new one has run
   on the book for a while.

## 5. Build

1. [ ] Build the installers:

   ```sh
   just build
   ```

   They are in `target/release/bundle/`: the `.deb` and AppImage on
   Kubuntu, the NSIS installer on Windows.

2. [ ] Install it. Help > About Kansha shows X.Y.Z.

## 6. Smoke test on the book

1. [ ] Sign in with the backup passphrase; the book opens.
2. [ ] File > Integrity Check is clean.
3. [ ] Account balances and Net Worth match the CSVs from §4.
4. [ ] In a bank register and an investment register, enter a
   transaction, then undo it.
5. [ ] Run a saved report. Save it as a PDF and print it from the
   viewer.
6. [ ] File > Back Up Now works, and Settings > Verify backup… passes
   on that backup.
7. [ ] Each change in this release's Appendix A entries works as
   described.

## 7. Record

1. [ ] Release notes: what changed, any ⚠ Schema change or ⚠ API
   change, known issues, coverage, and timings.
2. [ ] Commit, then tag vX.Y.Z in GitKraken and push the tag.
3. [ ] Sync the spec to the Claude Project copy.

# Phase 8 — Encryption, backup, restore, settings

Spec: 0.3.32 (design 0.3.30–0.3.31); changes since in the last section (now 0.3.37). App version 0.7.0. `just check` green.

**Exit criteria (spec §24):** restore drill passes
(`tests/integration/backup.rs::restore_drill`: back up, change the
book, compare, restore, reopen with the backup's passphrase, data
identical row for row). **Open:** Stan's hands-on run of setup,
conversion of the prototype database, backup on close, and restore in
the app; `just test` on Windows. Review findings for spec 0.4: done,
`prototype-review.md`.

**⚠ API change:** 18 new commands: `book_status`, `book_setup`,
`book_unlock`, `backup_now`, `backup_info`, `backup_manifest`,
`backup_verify`, `restore_open`, `restore_apply`, `restore_cancel`,
`passphrase_change`, `database_key_show`, `settings_get`,
`settings_set`, `appearance_get`, `appearance_set`, `pick_folder`,
`pick_backup_file`. `Dashboard.backup`; warning kind `backup`; IPC
error kinds `wrong_passphrase` and `locked`. `just bindings` run.
**No schema change** (settings use the `setting` table from 0001), so
the tithing columns are still there for the next migration.

New dependencies: `age =0.12.1`, `zip =7.2.0` (newest for MSRV 1.85),
`getrandom =0.2.17`, `flate2 =1.1.10` (both already under `age` and
`zip`), `tauri-plugin-dialog 2`; rusqlite feature `serialize`.

## Files

- `security.rs`: `Passphrase` (scrypt cost settable for tests),
  `DbKey` (32 random bytes, SQLCipher raw key), `PublicKey`,
  `PrivateKey`, `LockedPrivateKey`, `KeyFile` (create, unlock, change
  passphrase, for restore, read, atomic write).
- `persistence/mod.rs`: `Db::open_keyed` (no migration, so the caller
  backs up first), `needs_migration`, `snapshot` (`sqlcipher_export`
  into an attached `:memory:` database, then `serialize`; plaintext
  only in RAM), `from_snapshot` (deserialize, migrate),
  `export_keyed` (to a new encrypted file; foreign keys off during the
  copy). `sqlcipher_export` does not copy `application_id`; set by
  hand.
- `persistence/backup.rs`: transaction counts per account, last audit
  time.
- `backup/`: file names, `write` (snapshot → integrity check → gzip →
  age → zip → read back and compare), `read_manifest`/`inspect`
  (checks without the passphrase), `open` (with it), `target_folder`
  (BAK-030 fallback), `back_up` (the whole routine, records status),
  `compare` (BAK-075), `retention` (BAK-040).
- `book.rs`: `BookFiles`, `BookState` (new, unencrypted, locked,
  key missing), `create`, `convert`, `unlock`, `install_restore`,
  `change_passphrase`, `show_key`. Replacing files: staged as `.new`
  beside them, then renamed (database, then key file).
- `settings/`: `Settings` (every book setting, defaults, ranges),
  `load`/`save`, `stale_price_days`, `BackupStatus` (`backup.*` keys).
- `local_config.rs`: per-computer `config.json` (theme, font size,
  window geometry, recent books).
- `src-tauri/state.rs`: `AppState` starts locked (`Option<OpenBook>`);
  `backup`, `close_book` (backup on exit). `commands/book.rs`. Merges,
  price import, and lot seeding back up first. Window geometry saved
  on close request, applied at startup. Backup on close runs at
  `RunEvent::Exit`.
- Frontend: `StartScreen.svelte` (passphrase; setup in order: book,
  backup folder, passphrase with SECU-030 warning; key missing),
  `components/backup/` (Restore with comparison, Verify, Change
  passphrase, Database key), `state/booksettings.svelte.ts` (all book
  settings through Rust), `state/book.svelte.ts`, `shell/startup.ts`.
  `prefs.ts` (localStorage) deleted; settings, date format, investment
  views, and theme/font now come from Rust. Settings dialog: first day
  of week, integrity at startup, stale-price days, upcoming days,
  backup folder and retention, Verify/Change passphrase/Show key.
  File menu: Back Up Now, Restore…. Calendar honours the first day of
  week. Dashboard: last backup, last verification, backup warnings.

## Decisions made in the build (confirmed by Stan 2026-09-29)

- A snapshot with integrity problems is still backed up and counted;
  the dashboard warns. The spec said "passes the integrity check
  before it is encrypted"; refusing would stop all backups of a
  damaged book. (BAK-080 text updated.)
- A failed backup before a merge or import stops the merge or import.
- Restore installs the backup's key pair: afterwards the backup's
  passphrase opens the book.
- Backup names use UTC (the clock gives UTC; colons replaced).
- Restore matches accounts by ID; a renamed account is one row
  marked as differing.
- Comparison balances use ledger sign (liabilities negative), as the
  account list.
- The localStorage values from before Phase 8 are not carried over:
  the book starts with default settings (nav bar, date format, views).
- A stale-price setting replaces the fixed 7 days for holdings, the
  Investments screen, and dashboard warnings; per-security values
  still win.

## Performance (release build, 34,495 transactions, 30 MB book)

| Step | Time |
|---|---|
| Convert prototype DB (incl. scrypt) | 1.3 s |
| Backup (snapshot, integrity, gzip, encrypt, write, read back) | 1.2 s; zip 3 MB |
| Unlock (scrypt ~1 s by design) | 1.2 s |
| Open backup (scrypt, decrypt, integrity) | 1.8 s |
| Compare | 0.13 s |
| Install restore | 0.15 s |

## Known gaps

- Not run in the real app yet (GUI flows tested with mocks only):
  restore and reload, the folder picker, window geometry, the timed
  backup, Help > About. (Setup, conversion, and unlock Stan has run.)
- ~~SET-040 default lot method stays per account~~ Done in 0.4: the
  book's method is what a new investment account starts with.
- SQLCipher logs "hmac check failed" to stderr on a wrong key (not an
  error in Kansha).

Not gaps, by Stan's decision (2026-09-29): backup on close blocks exit
about 1 s on a large book; old plaintext pages of the converted
prototype file may remain on disk.

## Changes after the build (2026-09-29, spec 0.3.33–0.3.37)

Made while Stan tried the app; none changes a Phase 8 decision.

- **Status bar (UI-045, 0.3.33).** Messages that used to be modal
  windows now appear in a bar on the Accounts button row
  (`AccountBar.svelte`, state in `state/status.svelte.ts`). A note
  clears after 30 s; an alert flashes once a second and stays 60 s.
  Back Up Now reports there ("Backed up to …", flashing if the folder
  was missing, the data had integrity problems, or the backup failed);
  the "Back up now" modal and `dialogState.backupDone` are gone.
  The startup integrity check (INT-030) shows "Integrity check found no
  problems with the data." there, and opens the window only when it
  finds problems (the report it already has is passed in, so it does
  not run twice); File > Integrity Check always opens the window. Report
  CSV/PDF export ("Saved to …") uses the bar too. Left in their own
  windows: passphrase changed, backup verification, restore comparison,
  CSV import result.
- **Settings dialog layout.** The form overflowed the window: the
  "On startup open to" dropdown (one choice per account) stretched the
  right column. The columns are now `fit-content(60%) minmax(0, 1fr)`
  and dropdowns are capped to the dialog. Backup folder: its label,
  then the path, then Browse… and Use Downloads, each on its own line;
  the "A folder on this computer…" note is removed. Verify backup…,
  Change backup passphrase…, and Show database key… are one
  "Backup tools" dropdown with an Apply button. Checked in WebKitGTK at
  680 px wide.
- **Appearance commands.** `appearance_get` / `appearance_set` (Phase 8)
  carry a `font` field since 0.3.36 (SET-025): the font is a per-computer
  setting beside theme and size. ⚠ API change; bindings regenerated.
- **Backup file names (0.3.37).** `kansha-YYYYMMDD-HHMMSSZ-<kind>.zip`
  (was `kansha-backup-2026-09-29T18-30-12Z-<kind>.zip`). Old names are
  still read, so existing backups still restore and are pruned by the
  same rules; none was renamed. `parse_parts` reads both styles.
- **Timed backups (SET-050, 0.3.37).** New kind `timeout`. A change
  starts a timer (`backup/timer.rs`, `BackupTimer`); 5 minutes later
  (setting `backup_timeout_minutes`, 0 = off) a backup is made; any
  backup resets it. Retention (`retention.rs`): timeouts are outside the
  keep count; only the newest is kept, and only while no other kind
  (manual included) is newer. `AppState` marks a change after each
  successful `write` (and after an auto-entry that entered something) and
  clears it after any backup. The UI (`shell/timedbackup.ts`) asks
  `backup_timed_due` every 30 s; if due it shows "Timed backup
  starting…" in the status bar, calls `backup_timed_run`, then shows
  "Timed backup finished: …" (flashing for a missing folder, integrity
  problems, or a failure; after a failure it skips 10 checks, about
  5 minutes). Settings has "Back up after a change (minutes, 0 = off)".
  ⚠ API change: two commands, `Settings.backup_timeout_minutes`,
  `BackupKind::Timeout`; bindings regenerated.
- **Help > About Kansha (UI-047).** Enabled; a small dialog with the
  version from `app_version`.
- **App version 0.7.0** in `Cargo.toml`, `package.json`,
  `tauri.conf.json` (and the lock files); `version.test.ts` checks the
  three agree.

## Code review 2026-09-30 (spec 0.4.5)

Phase 8 re-read against the code. Fixed, with tests that failed first:

- `book::commit` renames `kansha.db.new`, then `kansha.key.new`. A
  crash between the two left a new database beside the old key file
  (restore: "the key file does not open"; first conversion: no key
  file and no backup yet). `book::recover`, called at startup in
  `src-tauri/src/lib.rs` before anything opens the book, finishes the
  swap when only the staged key file is left, and deletes the staged
  files when the staged database is still there (tests
  `recover_finishes_a_swap_stopped_after_the_database_was_renamed`,
  `recover_drops_staged_files_when_the_swap_never_started`).

~~Open: pruning counts every `kansha-…zip` in the folder, so two books
backing up to one folder prune each other's automatic backups.~~
Closed in spec 0.5: books are named and backups carry the book's name
(`books.md`).

Checked, no change: in-memory snapshot, integrity check, gzip, age,
zip, write to `.partial` with fsync then rename, read-back compare,
no overwrite, open checks and newer-schema refusal, retention (keep
last is at least 1), atomic key file write, backup before migration
on unlock, restore backs up first and fails safe before the swap, raw
hex database key in `PRAGMA key`.

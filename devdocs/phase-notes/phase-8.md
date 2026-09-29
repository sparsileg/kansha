# Phase 8 — Encryption, backup, restore, settings

Spec: 0.3.32 (design 0.3.30–0.3.31). `just check` green.

**Exit criteria (spec §24):** restore drill passes
(`tests/integration/backup.rs::restore_drill`: back up, change the
book, compare, restore, reopen with the backup's passphrase, data
identical row for row). **Open:** Stan's hands-on run of setup,
conversion of the prototype database, backup on close, and restore in
the app; review findings for spec 0.4; `just test` on Windows.

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

## Decisions made in the build (check with Stan)

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
  setup, conversion, unlock, backup on close, pickers, restore and
  reload, window geometry.
- SET-050 "schedule" (timed backups) not built; backups are on close,
  manual, and before migrations, merges, and imports.
- SET-040 default lot method stays per account (no book-wide setting).
- Help > About still disabled.
- SQLCipher logs "hmac check failed" to stderr on a wrong key (not an
  error in Kansha).
- Backup on close blocks exit ~1 s on a large book; no progress shown.
- Old plaintext pages of the converted prototype file may remain on
  disk (rename replaces the file; no secure wipe).

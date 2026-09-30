# Books — named books, File > New, Open, Recent, Rename

Date: 2026-09-30. Spec: 0.5 (UI-080). After Phase 8; closes the Phase
8 review's open finding (two books sharing a backup folder pruned each
other's automatic backups). `just check` green.

**⚠ API change:** new commands `book_new`, `book_open`, `book_rename`,
`book_recent`, `pick_book_file`; `book_setup` takes `name` and
`folder`; `BookStatus` gains `name` and `folder`; `Manifest.book`.
`just bindings` run. **No schema change.**

## Files

- `book.rs`: `validate_name`, `BookFiles::named`, `name`, `folder`;
  `create` refuses an existing key file; `rename` (marker
  `<new>.rename` holds the old name until both files are moved);
  `recover` finishes a marked rename before its swap checks.
- `backup/mod.rs`: `file_name(book, …)`, `parse_file_name(book, …)`,
  `parse_parts(book, …)`; `write` takes the `KeyFile` and the book
  name; `back_up` takes the book name; `Manifest.book` (optional,
  format still 1).
- `backup/retention.rs`: `prune_plan(book, …)`, `prune(folder, book, …)`.
- `local_config.rs`: `forget_book`.
- `src-tauri/src/state.rs`: `files` behind a mutex (`files()`,
  `set_files`); backups use the book's name.
- `src-tauri/src/lib.rs`: start book = `KANSHA_DB`, else the most
  recent book on disk (each recovered first), else `kansha.db` in the
  app data folder. Capability `core:window:allow-set-title`.
- `src-tauri/src/commands/book.rs`: the new commands above.
- UI: `lib/shell/books.ts` (switch, open, recent items),
  `components/books/NewBookModal.svelte`, `RenameBookModal.svelte`;
  File menu New…, Open…, Recent (filled in `App.svelte`), Rename
  Book…; StartScreen: book name and path on the passphrase screen,
  "Open another book…" and recent books, setup name and folder;
  RestoreModal shows the backup's book and flags another book's;
  window title `<name> — Kansha`.
- Tests: `tests/integration/backup.rs` (names, existing key file,
  manifest book, rename, interrupted rename), unit tests in
  `backup/mod.rs` (a backup belongs to one book) and
  `backup/retention.rs` (a book never prunes another's), `local_config`
  (forget); `App.smoke.test.ts` (setup name, File menu), menus test.

## Decisions

- Book name = database file name without `.db`; any folder (Stan).
  Names `[A-Za-z0-9_-]`, 1–40, starting with a letter or digit (Stan).
  File > Open accepts any `.db` file; its stem is used as is.
- Backups `<book>-YYYYMMDD-HHMMSSZ-<kind>.zip`. The existing book is
  `kansha`, so its backups kept their names; `kansha-backup-…` names
  (before 0.7.0) count as the `kansha` book's. The strict stamp keeps
  `barton` from reading `barton-2026-…` as its own.
- Switching books reloads the page after Rust has closed the old book
  (with a backup on close), so no store keeps the old book's data.
  File > New makes the new book first and closes the old one only if
  that worked.
- Rename asks for the passphrase: the book is closed for the rename
  and opened again, and the open book does not keep its database key.
  (The plan said no passphrase; changed during the build.) No backup is
  made for a rename: the data does not change, and old-name backups
  stay as they are.
- A backup of another book can be restored; the restore window says
  so, and the book keeps its own name.

## App identifier

Changed from `org.sparsile.kansha` to `tools.astryx.kansha`
(`src-tauri/tauri.conf.json`). Tauri names the default data folder
(`~/.local/share/<id>/`) and the config folder (`~/.config/<id>/`)
after it, so on Stan's computer `config.json` (theme, font, window,
recent books) moves by hand once; the book goes wherever he keeps it
(File > Open adds it to Recent). The first-run screen gained "Open an
existing book…" so a book elsewhere is reachable without a config file.

Books in a synced folder (pCloudSync): safe with one computer at a
time; close Kansha and let the sync finish before opening the book on
another computer; after a crash, reopen on the same computer first.
Keep backups in a different folder from the live book.

## Known gaps

- Not run in the real app yet: New, Open, Recent, Rename, the window
  title, the file pickers.
- Old-name backups after a rename are never pruned; delete them by
  hand.
- No way to delete a book or drop one from Recent other than it being
  gone from disk (then it shows greyed; opening it drops it).
- Per-book window geometry and more than one open book: later.

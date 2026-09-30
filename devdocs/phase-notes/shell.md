# UI shell

Built after Phase 4, from `devdocs/UI-conventions.md`. No API or schema
change. Spec follow-ups are listed in that document.

## What was built

- **Menu bar** (`components/shell/MenuBar.svelte`, contents in
  `shell/menus.ts`): File, Edit, Tools, Reports, Help. In-window, opens
  on click, switches on hover, keyboard: Down/Enter open, Up/Down skip
  greyed items, Left/Right switch menus, Esc closes. Greyed items show
  their reason ("Planned: Phase 8") as text and tooltip.
- **Navigation bar** (`NavBar.svelte`): buttons and their order come
  from the user's list (default: Home, Reminders with its badge = due,
  overdue, or awaiting review, Calendar, Reconcile and Investments
  greyed), plus the Search box at the right. Each icon has a text
  label.
- **Edit > Navigation Bar** (`NavBarModal.svelte`): two lists (available;
  on the bar in order) with Add, Remove, Up, Down, Save, Cancel, Reset.
  Available = Home, Investments view, every menu item, every account
  (`shell/navitems.ts`). Buttons and menu items run the same action by
  id (`shell/actions.ts`). Stored in the book (setting `nav_items`,
  Phase 8; was localStorage); ids of deleted accounts are dropped.
- **Account bar and panel** (`AccountBar.svelte`, `AccountPanel.svelte`,
  `AccountList.svelte`): one "Accounts" button with a triangle. Open:
  the triangle points down and clicking closes the panel. Closed: it
  points sideways and clicking drops the list down to pick an account;
  the drop-down has "Keep open" to bring
  the panel back. "Show closed accounts" is in the list. Open/closed and
  side (left or right) are kept between runs.
- **Views:** Accounts (Tools > Accounts: list with Open, Edit…, New
  account; close, reopen, and delete are in the account dialog),
  Reminders (the old Scheduled view, with a "Due and overdue" button),
  Manage opens on the tab a menu item names.
- **Search** (`views/Search.svelte`, Rust `ledger::search`, command
  `search_transactions`, **⚠ API change**): Enter in the navigation bar
  box shows matches for payee, memo, note, check number, category,
  account name, or an amount, newest first, capped at 200 with the total
  shown. Inside an account a "This account" checkbox limits it. Clicking
  a match opens the account on that day with the transaction selected
  (`registerState.goToTransaction`, as the other side of a transfer
  does). A transfer matches once per account. The register's own text
  filter box was removed; `RegisterQuery.text` stays in Rust, unused by
  the UI. Spec 0.3.7: UI-070, REG-040.
- **Settings dialog** (Edit > Settings): "On startup open to:", account
  list side. Theme and font size are pickers at the right end of the
  menu bar (`shell/ThemePicker.svelte`, passed to `MenuBar` as its
  children; 2026-09-29, spec 0.3.28).
- **Startup setting** (2026-09-28, spec 0.3.23; was "Home screen"):
  `shell/nav.ts` `startupChoices()` = Dashboard, Investments,
  Reminders, Calendar, Accounts, then every account. `STARTUP_VIEWS`
  and `STARTUP_PANELS` are `Record`s over `ViewId` and `PanelKind`, so
  a new view or panel does not compile until it is listed (label or
  `null`). `openStartup()` runs at startup (unless the user already
  navigated); anything it cannot open falls back to the dashboard.
  Still saved under the pref key `home`. The **Home button always opens
  the dashboard** (`goHome()`).
- **Register rows** (spec 0.3.23, REG-070): banking and investment
  registers stripe by row position (`alt`), not `nth-child`, so the
  today line and an open editor do not shift stripes. Future rows are
  italic and their alternate rows use `--future-alt` (light orange);
  not dimmed. Reconciled rows use `--reconciled-fg` (gray, 5:1 or
  better). Both variables are in the theme files (`src/css/themes/`).
- **Account status line** (register footer): transaction count and pager
  on the left; Available credit, Cleared, Current, Ending on the right.
  The Current/Ending line in the account header is gone.
- **View history:** `viewState` is a stack (view plus params) with
  `back()` and `forward()`; the arrows are not shown yet.
- **File > Exit** closes the window (`core:window:allow-close` added to
  `src-tauri/capabilities/default.json`; restart `just dev`).
- Removed: `AccountSelector`, the `accountNav` setting, the old top bar
  (Toggle theme, New account, Integrity check, mode buttons).

- **Settings dialog layout** (2026-09-28): labels right-aligned left of
  their controls, one line per setting (`display: contents` labels in a
  two-column grid).
- **Column gap** (2026-09-28): `--col-gap: 6px` in `src/css/base.css` is the least
  space between table columns. HTML cells get half each side from a
  zero-specificity `:where(th, td)` rule in `base.css` (tables with
  wider padding keep it); the register and its editor use
  `column-gap: var(--col-gap)`. `src/lib/columnGap.test.ts` fails on any
  grid or cell rule below it. 6px is a first try (Stan).
- **Themes and base font size** (2026-09-28, spec 0.3.26, SET-010,
  SET-020). Files created: `src/css/base.css` (named text sizes,
  `--col-gap`, page, buttons, scrollbars, focus, table cells; moved from
  `App.svelte`), `src/css/themes/{light,dark,classic}.css` (variables
  only, `[data-theme]`-scoped, all loaded by `main.ts`),
  `src/lib/themes.test.ts`. `theme.svelte.ts` puts `data-theme` and the
  base size on `<html>` (`applyTheme`, also before first paint);
  sizes 10–24 px, default 13 (a saved size stays). Every hard-coded
  component color became a variable; every em/px text size became a
  named rem size. Buttons are now themed everywhere (flat, not the GTK
  look). The register's sort column header is highlighted; the report
  page takes `data-theme="light"` (paper in every theme). Vitest runs
  with `css: true` so tests can read the CSS files.

## Decisions

- Startup opens the startup setting, not the last view.
- Clicking the account already open (in the list) returns to it without
  resetting its filters or sort.
- Preferences lived in localStorage (`state/prefs.ts`) until Phase 8,
  which stores them with the book (SET-070); the startup setting is
  per book.
- Help > About was greyed; it works since 0.3.37 (UI-047).
- Cleared stays in the status line (REG-060, needed for Phase 5).

## Known gaps

- At large base sizes in a narrow window the register's fixed columns
  (rem) no longer fit: the `fr` columns (Payee, Category, Memo)
  collapse and the entry row runs off the right edge. Seen at 24 px in
  a 1280 px window.
- The register's "Today" label sits over the balance of the row above
  it.
- Native checkboxes do not grow with the base size.

- Back/Forward arrows, Import, Export, File > New/Open, and Edit >
  Renaming are placeholders. (Back Up Now and Restore work since
  Phase 8; Edit > Undo since 0.4.)
- Status line shows the filtered count only, not "N of M".

## Proposed: account registers in the dock (not agreed)

Stan, 2026-09-29: Quicken 2013 has a dock bar and he expected account
views to minimize to it. Kansha's dock (UI-040) holds only reports and
the Calendar, Reminders, Accounts, and Reconcile windows; an account is
an ordinary view. Nothing is built; no spec change until agreed.

Proposal:

- A new window kind `account`, one window per account. Opening an
  account (account list, Accounts bar, search hit, transfer jump,
  startup setting) shows its window if open, else opens one. The
  WindowFrame gives it Minimize and Close; the dock label is the
  account name.
- Each account window keeps its own register state: filters, sort,
  scroll position, selection, and an entry row in progress.
  `registerState` is one shared instance today (`state/register.svelte.ts`);
  it becomes one instance per window. This is most of the work.
- Minimizing with an unsaved entry keeps it (the window is only
  hidden). Closing asks to save it or discard it, like a changed
  report.
- Investment accounts the same way (`InvestmentAccount`).
- The Home button and other views leave account windows in the dock,
  as for reports.

Open questions for Stan (how Quicken 2013 does it):

1. Do account registers go in the dock, or only reports and other
   windows?
2. How does a register get there: a minimize button, or by opening
   another account?
3. Can the same account be in the dock more than once?

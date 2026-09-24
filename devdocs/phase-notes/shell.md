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
  id (`shell/actions.ts`). Stored in localStorage as `navItems`; ids of
  deleted accounts are dropped.
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
- **Settings dialog** (Edit > Settings): theme, font size, home screen,
  account list side.
- **Home screen setting:** `dashboard` (still the Phase 0 placeholder),
  `calendar`, `scheduled` (Reminders), or `account:<id>`. Opened at
  startup (unless the user already navigated) and by the Home button.
- **Account status line** (register footer): transaction count and pager
  on the left; Available credit, Cleared, Current, Ending on the right.
  The Current/Ending line in the account header is gone.
- **View history:** `viewState` is a stack (view plus params) with
  `back()` and `forward()`; the arrows are not shown yet.
- **File > Exit** closes the window (`core:window:allow-close` added to
  `src-tauri/capabilities/default.json`; restart `just dev`).
- Removed: `AccountSelector`, the `accountNav` setting, the old top bar
  (Toggle theme, New account, Integrity check, mode buttons).

## Decisions

- Startup opens the home screen, not the last view.
- Clicking the account already open (in the list) returns to it without
  resetting its filters or sort.
- Preferences live in localStorage (`state/prefs.ts`) until the settings
  module stores them with the book (SET-070). The home screen will then
  be per book.
- Help > About is greyed (my choice; Help had no items specified).
- Cleared stays in the status line (REG-060, needed for Phase 5).

## Known gaps

- Back/Forward arrows, Reconcile, Investments, Reports,
  Backup, Restore, Import, Export, File > New/Open, Edit > Renaming are
  placeholders.
- Status line shows the filtered count only, not "N of M".
- The Dashboard is still the Phase 0 placeholder (Phase 7).
- Register still paginates (continuous scroll is deferred).
- Not hands-on tested by Stan yet.

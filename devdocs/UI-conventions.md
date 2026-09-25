# UI Conventions and Structure

- Title bar at top of window with name of app and a small logo
- Menu bar with following options: File, Edit, Tools, Reports, Help
- File has the following options: New, Open, Backup, Restore, Import,
  Export, Integrity Check, Exit
- Edit has: Settings, Renaming
- Tools has: Accounts, Calendar, Reminders, Categories, Tags,
  Reconcile
- Reports has: Saved, Investing, Balances, Spending, Taxes
- Under the menu bar is a Navigation Bar. This has backward and
  forward arrows (future history capability), and a space for
  buttons/icons that are "Quick Jump" to various actions such as
  Manager reminders, Reconcile current account, Show Investment view,
  and Calendar. On the far right side of the navigation bar is a
  search field that searches payee, category, memo, payment/deposit
  and will display a list of all records that match the search string
- Immediately under the Navigation Bar is a very thin bar that will,
  currently, only contain the Account List toggle.
- Under that will be the various views: accounts, calendar, etc.
- Each account view will have a bottom line that shows the number of
  transactions on the far left and then on the right side, it will
  show the Current Balance of the account (as of today) and the Ending
  Balance, which takes into account future transactions.

## Decisions

Agreed with Stan, 2026-09-24. The text above is Stan's outline; this
section records what was decided about it.

### Structure

- **Menu bar:** an in-window Svelte component, not native Tauri menus
  (themeable, same on every platform, testable with Vitest). It works
  with Tab, arrow keys, Enter and Esc (NFR-080). No Alt accelerators or
  other keyboard shortcuts for now; the OS often intercepts them. A few
  exceptions may come later.
- **Title bar:** the native OS title bar; name and logo come from the
  window title and icon. No custom window chrome.
- **Back and forward:** the view store becomes a history stack (each
  entry is a view plus its parameters, such as the account and the
  selection) so the arrows can be added without rework. Until they work
  they are not shown.
- **Search:** results are a plain list of matches, not a register.
  Clicking a match jumps to that transaction in its account. The search
  box in the navigation bar searches every account; inside an account a
  "This account" checkbox limits it to that account. The register's own
  text-search box is gone (the other register filters stay). Built:
  `search_transactions` and the Search view.
- **Menu items for features not built yet** (Backup, Restore, Import,
  Export, Reconcile, Investing reports, and so on) are shown greyed
  out, so the full target list stays visible. A tooltip names the phase
  that brings each one.

### Navigation bar contents

- The user chooses the buttons and their order in **Edit > Navigation
  Bar**, not by dragging. The dialog has two lists: everything
  available (Home, the investments view, every menu item, every
  account) on the left, and what is on the bar, in order, on the right,
  with Add, Remove, Up, and Down buttons. Save keeps the change, Cancel
  drops it, Reset restores the default.
- Any menu action can be a button, since buttons and menu items run the
  same action by id. Greyed items can be added; they stay greyed until
  built.
- The choice is a per-computer preference (localStorage) for now.

### Menu contents

- **Reminders** means the scheduled transactions. It replaces the
  working names Scheduled and Due. (The Due dialog and the Scheduled
  list are views of Reminders.)
- **Tools > Payees, Categories, Tags:** each opens the existing Manage
  view on that tab.
- **Tools > Accounts:** opens a list of accounts with management
  functions: New, Edit, Close and reopen, Delete, and so on.
- **Edit > Renaming:** kept in the menu, greyed out. Deferred: define
  what it renames and how it works.

### Books (File > New, Open)

- File > New and Open choose a specific database file. Stan keeps his
  own book and, separately, another person's, and opens the second one
  every few months. Almost always he is in his own.
- The app remembers the last book opened and starts in it. A recent
  books list under File is expected.
- A setting chooses the screen to open to (a specific account, the
  calendar, the investments view, or another view). It belongs to the
  book, since each book wants a different start screen.
- Opening a book means asking for its passphrase (SECU-020) unless it is
  kept in the OS keyring. Switching closes the current book first.

### Home screen and Due count

- **No separate Dashboard entry.** The app has a home screen, chosen by
  the per-book setting (a specific account, the calendar, the
  investments view, the Dashboard once it exists, the last view, or
  another view). A Home icon in the navigation bar goes to it.
- **Dashboard:** built in Phase 7 as specified (DSH-010: net worth with
  its breakdown, and this month's income, expenses, and net). It is one
  of the choices for the home screen, not a required screen.
- **Due count:** a small number on the Reminders icon in the navigation
  bar (due, overdue, or awaiting review), visible from any view.
- **One book open at a time.** Opening another book closes the current
  one.

### Spec follow-ups (next spec revision)

- SET-060 (startup behavior) becomes the per-book home screen setting.
- UI-020 (icon bar) and UI-010 (account selector) follow this document.
- Add requirements for the menu structure, search, and multiple books
  (one open at a time).

### Look and feel

Agreed with Stan, 2026-09-24.

- **Focus:** the field, dropdown, or button being worked on is
  unmistakable, the same everywhere: the theme's focus background and
  text color and a ring inside its edge. Light theme: dark text on
  yellow, blue ring. Dark theme: white text on deep blue, yellow ring.
  Selecting a field's text on focus keeps that look (a stronger shade
  of the background). A dropdown's open list uses the theme's plain
  colors.
- **Select on focus:** entering any text field selects what is in it,
  so typing replaces it.
- **Dates:** shown and typed in the format chosen in Settings:
  MM/DD/YYYY (default), DD/MM/YYYY, or YYYY-MM-DD. Logs and histories
  show `YYYY-MM-DDTHH:MM:SSZ`.
- **Register filter bar:** compact. Each box is as wide as its content,
  labels sit beside the boxes, and the row does not stretch with the
  window or line up with the register columns.
- **Register scrolling:** pagination goes; the register lazy-scrolls
  through all transactions (not yet built).
- **Reconcile view:** statement, totals, and buttons stay put while
  each item list scrolls on its own. The checkbox sits by the amount,
  and a click anywhere on a row toggles it. Credit card amounts appear
  as the statement prints them (balance owed positive).

## Status

The structure above is built (menu bar, navigation bar, account bar and
panel, status line, Settings). See `devdocs/phase-notes/shell.md`.

# Phase 4 — Scheduling and calendar

Spec: 0.3.5. Split into 4a (engine, persistence, IPC, scenarios) and 4b (Svelte UI), as in Phase 3.

## Status

| Sub-phase | State |
|---|---|
| 4a Recurrence engine, schedules, occurrences, IPC | Built. `just check` green. Uncommitted. |
| 4b UI: scheduled list, due-and-overdue dialog, calendar, "Schedule this" | Built. `just check` green (frontend tests added). Not yet hands-on tested by Stan. Uncommitted. |

**⚠ API change:** 15 new commands (below); `just bindings` run. **⚠ Schema change:** migration 0002 adds `schedule_occurrence.needs_review`.

## 4a — what was built

### Files

- `schedule/recurrence.rs`: `Recurrence`, `Frequency`, `WeekendRule`, `Dates` iterator, `due_date`, `describe`. Pure; no database.
- `schedule/mod.rs`: types (`Schedule`, `ScheduleFields`, `ScheduleLine`, `End`, `Occurrence`, `OccurrenceView`, `ScheduleRow`, `DayBalance`, enums).
- `schedule/service.rs`: `create`, `update`, `delete`, `from_entry`, `enter`, `skip`, `set_override`, `auto_enter_due`, `review_list`, `dismiss_review`, `list_rows`, `due_list`, `occurrences_between`, `projected_balances`.
- `persistence/schedules.rs`: repository. `migrations/0002_occurrence_review.sql`.
- `ledger`: `create_with_source` is `pub(crate)` so the scheduler can write `TxnSource::Schedule`. `ledger::create` still refuses the scheduler origin.
- `src-tauri/src/commands/schedule.rs`; `AppState::auto_enter`.
- Tests: recurrence unit tests, `tests/integration/schedule.rs` (31), enum test in `schema.rs`, migration 0002 test, scenarios `tests/scenarios/schedule/REC-001…011.toml` (runner gained `schedule`, `enter_occurrence`, `skip_occurrence`, `override_occurrence`, `auto_enter`, `expect.schedules`, `expect.occurrences`; documented in the scenarios README).

### IPC commands

`schedule_list`, `schedule_get`, `schedule_create`, `schedule_update`, `schedule_delete`, `schedule_from_txn`, `schedule_enter`, `schedule_skip`, `schedule_override`, `schedule_due_list`, `schedule_auto_enter`, `schedule_review_list`, `schedule_review_dismiss`, `calendar_occurrences`, `calendar_projection`.

Startup order for the UI: `schedule_auto_enter`, then show `schedule_review_list` (if any), then `schedule_due_list`.

### Decisions

- Nominal date identifies an occurrence; the weekend rule and one-time date only change when it is due. `schedule.next_due` stores the nominal date.
- Handled in order: only `next_due` can be entered or skipped. Keeps "# left" and `next_due` simple. The calendar shows later occurrences (`actionable = false`) and allows one-time edits on them.
- Skip uses up one of "# left" (as Quicken does). Confirmed by Stan; REC-030 now says "entered or skipped" (spec 0.3.6).
- Monthly with `day1` earlier than `start_date`'s day starts next month (series = pattern dates on or after `start_date`).
- Twice monthly with both days clamping to one date (30 and 31 in February) yields one occurrence.
- Series edit ("this and future"): continues after the last handled occurrence or at `start_date` if later; drops pending overrides; can revive an ended schedule.
- Amount override at entry or per occurrence only on single-line schedules. Splits: enter, then edit the split in the register.
- Single-line schedule tag becomes the entry-level tag (as the register does); split lines keep line tags.
- Estimated: `confirmation_required` unless an amount is given or `confirmed`. Never auto-entered.
- Auto-enter runs each occurrence in its own transaction; a failure (closed account) stops that schedule and is reported, others continue. Cap 2000 occurrences per schedule per run.
- Auto-entered occurrences carry `needs_review` until dismissed (REC-070 "flagged for review").
- Delete: hard delete if nothing references the schedule, else `status = deleted`.
- Projection: overdue pending items count on today. Window max 3660 days.

### Known gaps

- A transaction entered from a schedule cannot be deleted (occurrence FK); void it. Undoing an entry needs a "revert occurrence" operation.
- CAL-020 "show completed transactions" currently means entered and skipped occurrences; other register transactions in the month are not in `calendar_occurrences`. Decide in 4b whether to add a query.
- No holiday calendar (REC-050 is weekends only, as specified).
- Deleted payee/account merges: `payee_merge` already moves schedules; account and category merge paths for `schedule_line` were done in Phase 3 (not re-tested here).
- `describe()` text is English-only, fixed strings.
- UI (4b) not built at this point: Scheduled list, edit form, due-and-overdue dialog, review list, calendar, "Schedule this" menu item, plus Phase 3 carry-overs (show closed accounts toggle, account panel redesign). All built in 4b or in the UI shell (see `shell.md`).

## 4b — what was built

No API or schema change in 4b (uses the 4a commands).

### Files

- `lib/schedule/form.ts` (+ test): form draft ↔ `ScheduleFields`. Maps "Quarterly" and "Twice a year" onto monthly intervals 3 and 6. Parses typed dates and amounts. No recurrence or money math.
- `lib/format/date.ts`: `weekdayOf`, `monthStart`, `addMonths`, `monthLabel`, `monthGrid` (integer math, no JS `Date`). Test `calendar.test.ts`.
- `lib/state/schedule.svelte.ts`: list, due, review, startup sequence (auto-enter, load, open the due dialog if anything needs attention), `changed()` refreshes lists, balances, and the open register.
- `dialogState`: `due`, `schedule` (`newSchedule`, `editSchedule`).
- Components: `ScheduleModal` (create, edit, delete), `OccurrenceRow` (Enter, Skip, Edit…: enter with edits, or change this occurrence only, or undo the change), `DueDialog` (auto-entry failures, review list, due list).
- Views: `Scheduled` (REC-300 columns; click a row to edit), `Calendar` (month grid, account filter, show entered and skipped, projected balance per day, day panel, "New schedule on this date").
- Register context menu: "Schedule this…" (REC-140).
- Nav (superseded by the UI shell, `shell.md`): Scheduled, Calendar, Due (n), and a "Show closed accounts" checkbox were top-bar buttons. They are now the Reminders and Calendar quick jumps (Reminders carries the due count), and the checkbox is in the account list.
- Tests: `OccurrenceRow.test.ts`, `ScheduleModal.test.ts`, smoke test for Scheduled and Calendar.

### Decisions

- Startup opens the Due dialog automatically when anything is due, awaiting review, or failed.
- Enter for an estimated single-line schedule always opens the panel; the amount sent is the confirmation. Estimated splits use the standard confirmation dialog.
- Amount edits are disabled on split schedules (engine rule); edit the split in the register after entering.
- Payee on a schedule is chosen from existing payees (payees are only created by entering transactions).
- "Show entered and skipped" on the calendar shows scheduled occurrences only, not every register transaction (CAL-020 gap from 4a stands).
- Calendar cells show three chips, then "+n more"; click a day for all.

### Known gaps

- Not hands-on tested. Suggested checks: create monthly rent; open Due; Enter; Skip; edit one occurrence; auto schedule with a past start (startup review list); estimated amount; split schedule; calendar projection; "Schedule this" from a register row.
- No keyboard shortcuts in the calendar grid beyond Enter/Space on a day.
- Calendar does not open a transaction in its register.
- A transaction entered from a schedule cannot be deleted (4a gap).

## 4b revisions (Stan's first review)

**⚠ API change** (`just bindings` run; no schema change):
- `schedule_create` and `schedule_update` gain `payee_name` (found or created in the same transaction, as `entry_create`). The form types a payee name; new names are allowed.
- `schedule_enter` gains `payee_name`; `EnterEdits` gains `entry` (the whole transaction as edited). With an entry, `date` and `amount` are ignored and an estimate counts as confirmed. Its account must be the schedule's.
- New `schedule_prefill(schedule, due)` returns the entry an occurrence would become.

**UI changes**
- Schedule form: each line is Amount, Category, Memo, Tag on one row. "Add split line" is gone; the last line has a "Split" button with an icon that adds a line.
- Entering an occurrence always goes through the register (Due dialog "Enter", or a click on an actionable reminder in the calendar): the account opens, the new-entry row is prefilled, focus is on the amount. Saving calls `schedule_enter` with the edited entry, so the occurrence is recorded and linked. Esc cancels; the occurrence stays pending.
- Due dialog "Edit…" now only sets or undoes a one-time date or amount. Non-actionable (later) calendar reminders just select their day.
- Tests: `EntryEditor.occurrence.test.ts`; new engine tests for `prefill_entry` and enter-with-entry.

## 4b revisions (Stan's hands-on review)

Tests 1–12 passed. Changes from that review:

**⚠ API change** (`just bindings` run; no schema change): new `category_create_path(path, kind)` finds or creates a `Parent:Child` category, creating missing levels in one transaction. A new level under an existing parent takes the parent's kind; a new top-level one takes `kind`. Names match ignoring case. Test `create_path_finds_or_creates_each_level`.

**UI changes**
- **New category inline:** the category picker (`TargetCombo`, with `newKind`: expense for a payment, income for a deposit) offers "+ Create new … category" as the last row when the typed text names nothing. It asks for confirmation first, so a typo does not create a category. Works in the register entry row, split lines, and the schedule form.
- **Split total stays fixed:** tried "total follows the lines" and reverted it (it silently changed 184.23 to 191.23 for lines 1.33 + 189.90). The amount never changes on its own; the remainder updates live and saving needs zero. Test added.
- **Select on focus:** money fields select their whole value on focus or click (`lib/ui/selectOnFocus.ts`): Payment, Deposit, split and schedule line amounts, occurrence override, payee default amount, account rate and limit.
- **One memo:** a single-line schedule shows only the transaction memo; the line memo shows with two or more lines. A saved single line with only a line memo shows it as the transaction memo.
- **Calendar:** "+n more" is a button; the selected day shows all its items in its cell, and the day panel is sticky and scrolls into view. Overdue chips say "Overdue". Chip and day-panel payee/account text is smaller, one line, cut with "…" (full text in the tooltip).
- **Calendar day panel:** one line per item (date, payee, account, amount). Clicking an item opens `OccurrenceModal` with the flags and Enter, Skip, Edit…. The Due dialog still shows full rows.
- **Account button** in the top bar returned to the last selected account from any view. Superseded: the account list (panel or drop-down) does this now.
- **Colors (Stan is red-green colorblind):** `--bad` (vermilion) and `--good` (blue) tokens on `.app`, per theme, replace the old red and green. Split remainder shows ✓ or ✗; the invalid filter date outline is dashed.

### Known gaps
- **Placeholders:** the layout and the colors are still placeholders pending a real UI design and theme.
- Calendar is still a view, not a docked panel.
- The picker offers "Create" even for the name of the account being edited (which cannot be its own transfer target).
- The Due dialog still shows full rows; the calendar day panel shows one line per item with details in a modal.

## Changes after the Phase 4 commit

Phase 4 ended with the commit "Finish Phase 4". Work after it, driven by Stan's review and the UI design in `devdocs/UI-conventions.md`, is recorded in `devdocs/phase-notes/shell.md`. What it changed for Phase 4 features:

- **Scheduled is now "Reminders"** (spec wording: the scheduled transactions). The list view is headed Reminders and has a "Due and overdue (n)" button that opens the Due dialog. The Reminders quick-jump button carries the due count. The startup behavior (auto-enter, review list, Due dialog when anything needs attention) is unchanged.
- **Calendar** and **Reminders** are menu items (Tools) and quick-jump buttons the user can place (Edit > Navigation Bar). The old top bar is gone.
- **Home screen setting** can be Reminders or Calendar; the app opens there at startup, before the auto-enter step reports.
- **Account panel redesign** (a Phase 3 carry-over) and **Show closed accounts** are done, in the shell.
- **Search** (`search_transactions`, spec UI-070) replaced the register's text-filter box.
- **Spec 0.3.6:** REC-030 says "entered or skipped" (confirmed by Stan). **0.3.7:** UI-070; REG-040.
- **⚠ API changes since the Phase 4 commit:** `search_transactions` only. No schema change.

# Phase 4 — Scheduling and calendar

Spec: 0.3.5. Split into 4a (engine, persistence, IPC, scenarios) and 4b (Svelte UI), as in Phase 3.

## Status

| Sub-phase | State |
|---|---|
| 4a Recurrence engine, schedules, occurrences, IPC | Built. `just check` green. Uncommitted. |
| 4b UI: scheduled list, due-and-overdue dialog, calendar, "Schedule this" | Not started. |

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
- Skip uses up one of "# left" (as Quicken does). Spec REC-030 says "entered"; the spec text stands, the behavior is skip-counts. **Stan: confirm.**
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
- UI (4b) not built: Scheduled list, edit form, due-and-overdue dialog, review list, calendar, "Schedule this" menu item, plus Phase 3 carry-overs (show closed accounts toggle, account panel redesign).

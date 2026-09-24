//! Scheduled and recurring transactions (REC-010 … REC-160, CAL-010 …
//! CAL-050).
//!
//! A [`Schedule`] is a transaction template plus a [`Recurrence`] and end
//! condition. Only the *next* occurrence is stored (`next_due`, a nominal
//! date); later ones are generated from the recurrence. An occurrence
//! gets a row (`Occurrence`) only when acted on or edited individually
//! (REC-110): entered, skipped, or given a one-time date or amount.
//!
//! Rules:
//!
//! - Occurrences are handled in order: only the schedule's `next_due`
//!   can be entered or skipped. Each enter or skip uses up one occurrence
//!   ("# left", REC-030) and moves `next_due` to the next nominal date.
//! - The nominal date identifies an occurrence. The date it is actually
//!   due is the nominal date after the weekend rule (REC-050), or a
//!   one-time override date.
//! - Amounts are in the main account's register sign, like an
//!   [`Entry`](crate::ledger::Entry): a schedule's amount is the sum of
//!   its lines. (The stored `schedule_line.amount` uses posting sign, the
//!   negative of this.)
//! - Estimated amounts (REC-060) must be confirmed on entry, and are
//!   never auto-entered.
//! - Auto-entered occurrences are flagged for review until dismissed
//!   (REC-070), and appear in the due list's companion review list.

mod recurrence;
mod service;

pub use recurrence::{Dates, Frequency, Recurrence, WeekendRule, add_days};
pub use service::{
    AutoEnterFailure, AutoEnterReport, EnterEdits, Entered, auto_enter_due, create, delete,
    dismiss_review, due_list, enter, from_entry, list_rows, occurrences_between,
    projected_balances, review_list, set_override, skip, update,
};

use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;
use crate::categories::{PayeeId, TagId};
use crate::date::{Date, Timestamp};
use crate::error::{Error, Result};
use crate::ledger::{Target, TxnId};
use crate::money::Money;
use crate::text_enum::text_enum;

/// Row ID of a schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent)]
pub struct ScheduleId(pub i64);

text_enum! {
    /// Fixed amounts enter as scheduled; estimated ones are confirmed on
    /// entry (REC-060).
    pub enum AmountType {
        Fixed = "fixed",
        Estimated = "estimated",
    }
}

text_enum! {
    /// Remind: the user enters each occurrence. Auto: entered on the due
    /// date and flagged for review (REC-070).
    pub enum EntryMode {
        Remind = "remind",
        Auto = "auto",
    }
}

text_enum! {
    /// Where a schedule stands.
    pub enum ScheduleStatus {
        Active = "active",
        /// Ran out of occurrences (end date passed or "# left" reached 0).
        Ended = "ended",
        Deleted = "deleted",
    }
}

text_enum! {
    /// What became of an occurrence (REC-110). `Pending` rows carry a
    /// one-time override.
    pub enum OccurrenceStatus {
        Pending = "pending",
        Entered = "entered",
        Skipped = "skipped",
    }
}

/// When a schedule stops (REC-030).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum End {
    Never,
    /// Occurrences dated after `date` do not happen.
    OnDate {
        date: Date,
    },
    /// `count` occurrences remain ("# left"); entering or skipping one
    /// uses it up.
    AfterCount {
        count: i64,
    },
}

/// One line of the other side of a scheduled transaction (REC-150).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ScheduleLine {
    /// A category, or the other account of a transfer.
    pub target: Target,
    /// Same sign as the schedule's amount, like an entry line.
    pub amount: Money,
    pub memo: String,
    pub tag: Option<TagId>,
}

/// Everything the user sets on a schedule (REC-010).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ScheduleFields {
    /// The account whose register the transaction goes in.
    pub account: AccountId,
    pub payee: Option<PayeeId>,
    pub memo: String,
    pub amount_type: AmountType,
    /// At least one. One line is a simple transaction.
    pub lines: Vec<ScheduleLine>,
    pub recurrence: Recurrence,
    pub end: End,
    /// Days before the due date it shows in the due list.
    pub remind_days: i64,
    pub mode: EntryMode,
}

impl ScheduleFields {
    /// The scheduled amount: the sum of the lines.
    pub fn amount(&self) -> Result<Money> {
        self.lines.iter().try_fold(Money::ZERO, |acc, l| {
            acc.checked_add(l.amount)
                .ok_or(Error::Overflow("schedule amount"))
        })
    }
}

/// A stored schedule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Schedule {
    pub id: ScheduleId,
    pub fields: ScheduleFields,
    /// Nominal date of the next occurrence; `None` once ended or deleted.
    pub next_due: Option<Date>,
    pub status: ScheduleStatus,
    pub created_at: Timestamp,
}

impl Schedule {
    /// The next occurrence's due date after the weekend rule (ignores a
    /// one-time override).
    pub fn next_due_date(&self) -> Option<Date> {
        self.next_due.map(|n| self.fields.recurrence.due_date(n))
    }
}

/// A stored occurrence row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Occurrence {
    pub id: i64,
    pub schedule: ScheduleId,
    /// Nominal date.
    pub due_date: Date,
    pub status: OccurrenceStatus,
    pub override_date: Option<Date>,
    /// The main account's amount, when changed for this occurrence only.
    pub override_amount: Option<Money>,
    pub txn: Option<TxnId>,
    pub needs_review: bool,
}

/// One occurrence as lists and the calendar show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct OccurrenceView {
    pub schedule: ScheduleId,
    /// Nominal date: identifies the occurrence.
    pub nominal: Date,
    /// When it is due: weekend rule or one-time override applied.
    pub date: Date,
    /// The main account's amount, override applied.
    pub amount: Money,
    pub status: OccurrenceStatus,
    pub account: AccountId,
    pub payee: Option<PayeeId>,
    pub estimated: bool,
    pub mode: EntryMode,
    /// A one-time date or amount is set.
    pub overridden: bool,
    /// The transaction, once entered.
    pub txn: Option<TxnId>,
    pub needs_review: bool,
    /// Pending and dated before today.
    pub overdue: bool,
    /// Only the schedule's next occurrence can be entered or skipped.
    pub actionable: bool,
}

/// A row of the Scheduled Transactions list (REC-300).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ScheduleRow {
    pub schedule: Schedule,
    /// "How often" text.
    pub how_often: String,
    pub amount: Money,
    /// Next due date, weekend rule applied.
    pub due_date: Option<Date>,
    /// "# left" for an after-count schedule.
    pub left: Option<i64>,
}

/// Projected balance at the end of a day (CAL-050).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DayBalance {
    pub date: Date,
    pub balance: Money,
}

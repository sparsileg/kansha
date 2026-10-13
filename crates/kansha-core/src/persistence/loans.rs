//! Loans (LOAN-090): the `loan`, `loan_rate`, and `loan_other` tables,
//! keyed by the loan's account.

use rusqlite::{Connection, OptionalExtension, Row, named_params, params};

use super::Tx;
use super::audit::{self, AuditAction, AuditEntity};
use crate::accounts::AccountId;
use crate::error::Result;
use crate::loan::{Loan, LoanFields, LoanOther, LoanRate};
use crate::schedule::ScheduleId;

const COLUMNS: &str = "account_id, loan_type, opening_date, original_balance, length_months,
     compounding, frequency, day1, day2, payment, extra_principal, lender_id,
     paying_account_id, interest_category_id, schedule_id";

fn from_row(r: &Row<'_>) -> rusqlite::Result<Loan> {
    Ok(Loan {
        fields: LoanFields {
            account: r.get("account_id")?,
            loan_type: r.get("loan_type")?,
            opening_date: r.get("opening_date")?,
            original_balance: r.get("original_balance")?,
            length_months: r.get("length_months")?,
            compounding: r.get("compounding")?,
            frequency: r.get("frequency")?,
            day1: r.get("day1")?,
            day2: r.get("day2")?,
            rates: Vec::new(),
            payment: r.get("payment")?,
            extra_principal: r.get("extra_principal")?,
            others: Vec::new(),
            lender: r.get("lender_id")?,
            paying_account: r.get("paying_account_id")?,
            interest_category: r.get("interest_category_id")?,
        },
        schedule: r.get("schedule_id")?,
    })
}

fn load_lists(conn: &Connection, loan: &mut Loan) -> Result<()> {
    let id = loan.fields.account;
    let mut stmt = conn.prepare_cached(
        "SELECT effective_date, rate FROM loan_rate WHERE loan_id = ?1 ORDER BY effective_date",
    )?;
    loan.fields.rates = stmt
        .query_map([id], |r| {
            Ok(LoanRate {
                effective: r.get(0)?,
                rate: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut stmt = conn.prepare_cached(
        "SELECT category_id, amount FROM loan_other WHERE loan_id = ?1 ORDER BY line_no",
    )?;
    loan.fields.others = stmt
        .query_map([id], |r| {
            Ok(LoanOther {
                category: r.get(0)?,
                amount: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(())
}

fn one(conn: &Connection, filter: &str, id: i64) -> Result<Option<Loan>> {
    let sql = format!("SELECT {COLUMNS} FROM loan WHERE {filter} = ?1");
    let Some(mut loan) = conn
        .prepare_cached(&sql)?
        .query_row([id], from_row)
        .optional()?
    else {
        return Ok(None);
    };
    load_lists(conn, &mut loan)?;
    Ok(Some(loan))
}

/// The loan on `account`, if it has terms.
pub fn get(conn: &Connection, account: AccountId) -> Result<Option<Loan>> {
    one(conn, "account_id", account.0)
}

/// The loan that owns `schedule`, if one does.
pub fn for_schedule(conn: &Connection, schedule: ScheduleId) -> Result<Option<Loan>> {
    one(conn, "schedule_id", schedule.0)
}

fn write_lists(tx: &Tx<'_>, f: &LoanFields) -> Result<()> {
    let conn = tx.conn();
    conn.execute("DELETE FROM loan_rate WHERE loan_id = ?1", [f.account])?;
    conn.execute("DELETE FROM loan_other WHERE loan_id = ?1", [f.account])?;
    for r in &f.rates {
        conn.execute(
            "INSERT INTO loan_rate (loan_id, effective_date, rate) VALUES (?1, ?2, ?3)",
            params![f.account, r.effective, r.rate],
        )?;
    }
    for (line, o) in (1_i64..).zip(&f.others) {
        conn.execute(
            "INSERT INTO loan_other (loan_id, line_no, category_id, amount)
             VALUES (?1, ?2, ?3, ?4)",
            params![f.account, line, o.category, o.amount],
        )?;
    }
    Ok(())
}

/// Add or replace the loan's terms, keeping its schedule. Checks are the
/// service's; the schema's CHECKs back them up.
pub fn put(tx: &Tx<'_>, f: &LoanFields) -> Result<Loan> {
    let conn = tx.conn();
    let before = get(conn, f.account)?;
    conn.execute(
        "INSERT INTO loan (account_id, loan_type, opening_date, original_balance, length_months,
             compounding, frequency, day1, day2, payment, extra_principal, lender_id,
             paying_account_id, interest_category_id)
         VALUES (:account, :type, :opening, :original, :months, :compounding, :frequency,
             :day1, :day2, :payment, :extra, :lender, :paying, :interest)
         ON CONFLICT (account_id) DO UPDATE SET
             loan_type = excluded.loan_type, opening_date = excluded.opening_date,
             original_balance = excluded.original_balance,
             length_months = excluded.length_months, compounding = excluded.compounding,
             frequency = excluded.frequency, day1 = excluded.day1, day2 = excluded.day2,
             payment = excluded.payment, extra_principal = excluded.extra_principal,
             lender_id = excluded.lender_id, paying_account_id = excluded.paying_account_id,
             interest_category_id = excluded.interest_category_id",
        named_params! {
            ":account": f.account,
            ":type": f.loan_type,
            ":opening": f.opening_date,
            ":original": f.original_balance,
            ":months": f.length_months,
            ":compounding": f.compounding,
            ":frequency": f.frequency,
            ":day1": f.day1,
            ":day2": f.day2,
            ":payment": f.payment,
            ":extra": f.extra_principal,
            ":lender": f.lender,
            ":paying": f.paying_account,
            ":interest": f.interest_category,
        },
    )?;
    write_lists(tx, f)?;
    let after = get(conn, f.account)?.ok_or(crate::error::Error::NotFound {
        entity: "loan",
        id: f.account.0,
    })?;
    if before.as_ref() != Some(&after) {
        let action = if before.is_some() {
            AuditAction::Update
        } else {
            AuditAction::Create
        };
        audit::record(
            tx,
            AuditEntity::Loan,
            f.account.0,
            action,
            before.as_ref(),
            Some(&after),
        )?;
    }
    Ok(after)
}

/// Link the loan to its payment reminder (the schedule's own audit entry
/// records the schedule).
pub fn set_schedule(tx: &Tx<'_>, account: AccountId, schedule: Option<ScheduleId>) -> Result<()> {
    tx.conn().execute(
        "UPDATE loan SET schedule_id = ?2 WHERE account_id = ?1",
        params![account, schedule],
    )?;
    Ok(())
}

//! Account repository (ACCT-100 … ACCT-240).

use rusqlite::{Connection, OptionalExtension, Row, named_params};

use super::Tx;
use super::audit::{self, AuditAction, AuditEntity};
use crate::accounts::{
    Account, AccountFields, AccountId, AccountStatus, InvestmentSettings, OtherAssetSettings,
};
use crate::date::Date;
use crate::error::{Error, Result};

const COLUMNS: &str = "id, name, type, account_group, tax_treatment, description, institution,
    account_number, contact_phone, home_url, notes, opening_date, show_in_bar, show_in_list,
    sort_order, interest_rate, credit_limit, account_subtype, cash_mode, linked_cash_account_id,
    mmf_mode, default_lot_method, asset_subtype, linked_liability_account_id, status,
    closed_date, created_at";

fn from_row(r: &Row<'_>) -> rusqlite::Result<Account> {
    let investment = match r.get::<_, Option<crate::accounts::CashMode>>("cash_mode")? {
        Some(cash_mode) => Some(InvestmentSettings {
            subtype: r.get("account_subtype")?,
            cash_mode,
            linked_cash_account: r.get("linked_cash_account_id")?,
            mmf_mode: r.get("mmf_mode")?,
            default_lot_method: r.get("default_lot_method")?,
        }),
        None => None,
    };
    let other_asset = match r.get::<_, Option<crate::accounts::AssetSubtype>>("asset_subtype")? {
        Some(subtype) => Some(OtherAssetSettings {
            subtype,
            linked_liability: r.get("linked_liability_account_id")?,
        }),
        None => None,
    };
    Ok(Account {
        id: r.get("id")?,
        fields: AccountFields {
            name: r.get("name")?,
            account_type: r.get("type")?,
            group: r.get("account_group")?,
            tax_treatment: r.get("tax_treatment")?,
            description: r.get("description")?,
            institution: r.get("institution")?,
            account_number: r.get("account_number")?,
            contact_phone: r.get("contact_phone")?,
            home_url: r.get("home_url")?,
            notes: r.get("notes")?,
            opening_date: r.get("opening_date")?,
            show_in_bar: r.get("show_in_bar")?,
            show_in_list: r.get("show_in_list")?,
            sort_order: r.get("sort_order")?,
            interest_rate: r.get("interest_rate")?,
            credit_limit: r.get("credit_limit")?,
            investment,
            other_asset,
        },
        status: r.get("status")?,
        closed_date: r.get("closed_date")?,
        created_at: r.get("created_at")?,
    })
}

/// Structural checks with readable messages. The schema's CHECKs are the
/// backstop; these catch the same mistakes before SQLite does.
fn validate(f: &AccountFields) -> Result<()> {
    use crate::accounts::{AccountType as T, CashMode};
    let t = f.account_type;
    if f.name.trim().is_empty() {
        return Err(Error::Invalid("account name is required".into()));
    }
    if t.is_investment() != f.investment.is_some() {
        return Err(Error::Invalid(format!(
            "investment settings are {} for a {t} account",
            if t.is_investment() {
                "required"
            } else {
                "not allowed"
            }
        )));
    }
    let linked_mismatch = f.investment.as_ref().is_some_and(|inv| {
        (inv.cash_mode == CashMode::Linked) != inv.linked_cash_account.is_some()
    });
    if linked_mismatch {
        return Err(Error::Invalid(
            "a linked cash account is required exactly when cash mode is linked".into(),
        ));
    }
    if (t == T::OtherAsset) != f.other_asset.is_some() {
        return Err(Error::Invalid(format!(
            "other-asset settings are {} for a {t} account",
            if t == T::OtherAsset {
                "required"
            } else {
                "not allowed"
            }
        )));
    }
    if f.interest_rate.is_some() && !matches!(t, T::Checking | T::Savings | T::MoneyMarket) {
        return Err(Error::Invalid(format!(
            "a {t} account has no interest rate"
        )));
    }
    if f.interest_rate.is_some_and(|r| r.is_negative()) {
        return Err(Error::Invalid("interest rate cannot be negative".into()));
    }
    if f.credit_limit.is_some() && t != T::CreditCard {
        return Err(Error::Invalid(format!("a {t} account has no credit limit")));
    }
    if f.credit_limit.is_some_and(|m| m.is_negative()) {
        return Err(Error::Invalid("credit limit cannot be negative".into()));
    }
    Ok(())
}

/// Create an account.
pub fn insert(tx: &Tx<'_>, f: &AccountFields) -> Result<Account> {
    validate(f)?;
    let inv = f.investment.as_ref();
    let oa = f.other_asset.as_ref();
    tx.conn().execute(
        "INSERT INTO account (name, type, account_group, tax_treatment, description,
             institution, account_number, contact_phone, home_url, notes, opening_date,
             show_in_bar, show_in_list, sort_order, interest_rate, credit_limit,
             account_subtype, cash_mode, linked_cash_account_id, mmf_mode, default_lot_method,
             asset_subtype, linked_liability_account_id, created_at)
         VALUES (:name, :type, :grp, :tax, :description, :institution, :number, :phone, :url,
             :notes, :opening, :bar, :list, :sort, :rate, :limit, :subtype, :cash_mode,
             :linked_cash, :mmf, :lot, :asset_subtype, :linked_liability, :created_at)",
        named_params! {
            ":name": f.name.trim(),
            ":type": f.account_type,
            ":grp": f.group,
            ":tax": f.tax_treatment,
            ":description": f.description,
            ":institution": f.institution,
            ":number": f.account_number,
            ":phone": f.contact_phone,
            ":url": f.home_url,
            ":notes": f.notes,
            ":opening": f.opening_date,
            ":bar": f.show_in_bar,
            ":list": f.show_in_list,
            ":sort": f.sort_order,
            ":rate": f.interest_rate,
            ":limit": f.credit_limit,
            ":subtype": inv.and_then(|i| i.subtype.as_deref()),
            ":cash_mode": inv.map(|i| i.cash_mode),
            ":linked_cash": inv.and_then(|i| i.linked_cash_account),
            ":mmf": inv.map(|i| i.mmf_mode),
            ":lot": inv.map(|i| i.default_lot_method),
            ":asset_subtype": oa.map(|o| o.subtype),
            ":linked_liability": oa.and_then(|o| o.linked_liability),
            ":created_at": tx.now(),
        },
    )?;
    let account = get(tx.conn(), AccountId(tx.conn().last_insert_rowid()))?;
    audit::record::<(), _>(
        tx,
        AuditEntity::Account,
        account.id.0,
        AuditAction::Create,
        None,
        Some(&account),
    )?;
    Ok(account)
}

/// One account by ID.
pub fn get(conn: &Connection, id: AccountId) -> Result<Account> {
    find(conn, id)?.ok_or(Error::NotFound {
        entity: "account",
        id: id.0,
    })
}

/// One account by ID, or `None`.
pub fn find(conn: &Connection, id: AccountId) -> Result<Option<Account>> {
    let sql = format!("SELECT {COLUMNS} FROM account WHERE id = ?1");
    Ok(conn
        .prepare_cached(&sql)?
        .query_row([id], from_row)
        .optional()?)
}

/// All accounts, open and closed, in display order (group, sort order, name).
pub fn list(conn: &Connection) -> Result<Vec<Account>> {
    let sql = format!(
        "SELECT {COLUMNS} FROM account ORDER BY account_group, sort_order, name COLLATE NOCASE, id"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Replace an account's editable fields. The account type is fixed at
/// creation.
pub fn update(tx: &Tx<'_>, id: AccountId, f: &AccountFields) -> Result<Account> {
    validate(f)?;
    let before = get(tx.conn(), id)?;
    if before.fields.account_type != f.account_type {
        return Err(Error::Invalid(format!(
            "account type cannot change ({} to {})",
            before.fields.account_type, f.account_type
        )));
    }
    let inv = f.investment.as_ref();
    let oa = f.other_asset.as_ref();
    tx.conn().execute(
        "UPDATE account SET name = :name, account_group = :grp, tax_treatment = :tax,
             description = :description, institution = :institution, account_number = :number,
             contact_phone = :phone, home_url = :url, notes = :notes, opening_date = :opening,
             show_in_bar = :bar, show_in_list = :list, sort_order = :sort, interest_rate = :rate,
             credit_limit = :limit, account_subtype = :subtype, cash_mode = :cash_mode,
             linked_cash_account_id = :linked_cash, mmf_mode = :mmf, default_lot_method = :lot,
             asset_subtype = :asset_subtype, linked_liability_account_id = :linked_liability
         WHERE id = :id",
        named_params! {
            ":id": id,
            ":name": f.name.trim(),
            ":grp": f.group,
            ":tax": f.tax_treatment,
            ":description": f.description,
            ":institution": f.institution,
            ":number": f.account_number,
            ":phone": f.contact_phone,
            ":url": f.home_url,
            ":notes": f.notes,
            ":opening": f.opening_date,
            ":bar": f.show_in_bar,
            ":list": f.show_in_list,
            ":sort": f.sort_order,
            ":rate": f.interest_rate,
            ":limit": f.credit_limit,
            ":subtype": inv.and_then(|i| i.subtype.as_deref()),
            ":cash_mode": inv.map(|i| i.cash_mode),
            ":linked_cash": inv.and_then(|i| i.linked_cash_account),
            ":mmf": inv.map(|i| i.mmf_mode),
            ":lot": inv.map(|i| i.default_lot_method),
            ":asset_subtype": oa.map(|o| o.subtype),
            ":linked_liability": oa.and_then(|o| o.linked_liability),
        },
    )?;
    let after = get(tx.conn(), id)?;
    if after != before {
        audit::record(
            tx,
            AuditEntity::Account,
            id.0,
            AuditAction::Update,
            Some(&before),
            Some(&after),
        )?;
    }
    Ok(after)
}

/// Mark an account closed as of `date` (ACCT-210). The zero-balance rule
/// needs the ledger and is enforced by the accounts service (Phase 2).
pub fn close(tx: &Tx<'_>, id: AccountId, date: Date) -> Result<Account> {
    set_status(tx, id, AccountStatus::Closed, Some(date))
}

/// Reopen a closed account.
pub fn reopen(tx: &Tx<'_>, id: AccountId) -> Result<Account> {
    set_status(tx, id, AccountStatus::Open, None)
}

fn set_status(
    tx: &Tx<'_>,
    id: AccountId,
    status: AccountStatus,
    closed_date: Option<Date>,
) -> Result<Account> {
    let before = get(tx.conn(), id)?;
    if before.status == status {
        return Err(Error::Invalid(format!(
            "account {:?} is already {status}",
            before.fields.name
        )));
    }
    tx.conn().execute(
        "UPDATE account SET status = ?2, closed_date = ?3 WHERE id = ?1",
        rusqlite::params![id, status, closed_date],
    )?;
    let after = get(tx.conn(), id)?;
    let action = match status {
        AccountStatus::Closed => AuditAction::Close,
        AccountStatus::Open => AuditAction::Reopen,
    };
    audit::record(
        tx,
        AuditEntity::Account,
        id.0,
        action,
        Some(&before),
        Some(&after),
    )?;
    Ok(after)
}

/// Delete an account that nothing references (ACCT-220). An account with
/// postings, schedules, lots, or links from other accounts is `InUse`;
/// close it instead.
pub fn delete(tx: &Tx<'_>, id: AccountId) -> Result<()> {
    let before = get(tx.conn(), id)?;
    tx.conn()
        .execute("DELETE FROM account WHERE id = ?1", [id])
        .map_err(|e| in_use_or(e.into(), "account", id.0))?;
    audit::record::<_, ()>(
        tx,
        AuditEntity::Account,
        id.0,
        AuditAction::Delete,
        Some(&before),
        None,
    )?;
    Ok(())
}

/// Map a foreign-key failure on DELETE to [`Error::InUse`].
pub(super) fn in_use_or(e: Error, entity: &'static str, id: i64) -> Error {
    match e {
        Error::Constraint(msg) if msg.contains("FOREIGN KEY") => Error::InUse { entity, id },
        other => other,
    }
}

//! JSON contract for IPC: the shapes the frontend sends and receives are
//! the ones in `src/lib/types/bindings.ts` (D-120). Amounts and dates are
//! strings, never numbers (NFR-030, NFR-090); enums are snake_case
//! strings; tagged unions carry `kind`.

use kansha_core::accounts::{AccountFields, AccountType};
use kansha_core::ledger::{Cleared, Counterpart, Entry, RegisterQuery, Target};
use serde_json::json;

#[test]
fn entry_deserializes_from_the_shape_typescript_sends() {
    let entry: Entry = serde_json::from_value(json!({
        "account": 1,
        "date": "2026-01-05",
        "payee": null,
        "check_num": "1001",
        "memo": "",
        "notes": "",
        "amount": "-250.00",
        "cleared": "cleared",
        "tags": [3],
        "lines": [
            { "target": { "kind": "category", "id": 10 }, "amount": "-200.00",
              "memo": "", "cleared": "unmarked", "tags": [] },
            { "target": { "kind": "account", "id": 2 }, "amount": "-50.00",
              "memo": "", "cleared": "unmarked", "tags": [] }
        ]
    }))
    .unwrap();
    assert_eq!(entry.amount.cents(), -25_000);
    assert_eq!(entry.cleared, Cleared::Cleared);
    assert!(matches!(entry.lines[0].target, Target::Category(c) if c.0 == 10));
    assert!(matches!(entry.lines[1].target, Target::Account(a) if a.0 == 2));

    // And back, byte for byte.
    let back = serde_json::to_value(&entry).unwrap();
    assert_eq!(back["amount"], "-250.00");
    assert_eq!(
        back["lines"][0]["target"],
        json!({ "kind": "category", "id": 10 })
    );
    assert_eq!(back["date"], "2026-01-05");
}

#[test]
fn bad_amounts_dates_and_numbers_are_rejected() {
    let base = |amount: serde_json::Value, date: &str| {
        json!({
            "account": 1, "date": date, "payee": null, "check_num": "", "memo": "",
            "notes": "", "amount": amount, "cleared": "unmarked", "tags": [], "lines": []
        })
    };
    for (amount, date) in [
        (json!(-250.0), "2026-01-05"),     // a JSON number, not a string
        (json!("1,000.00"), "2026-01-05"), // separators
        (json!("-250.00"), "2026-02-30"),  // not a date
        (json!("-250.00"), "01/05/2026"),  // wrong form
    ] {
        let r = serde_json::from_value::<Entry>(base(amount.clone(), date));
        assert!(r.is_err(), "accepted {amount} {date}");
    }
    let r = serde_json::from_value::<Entry>({
        let mut v = base(json!("1.00"), "2026-01-05");
        v["cleared"] = json!("Cleared");
        v
    });
    assert!(r.is_err());
}

#[test]
fn account_fields_and_query_round_trip() {
    let fields = AccountFields::new("Brokerage", AccountType::Brokerage);
    let v = serde_json::to_value(&fields).unwrap();
    assert_eq!(v["account_type"], "brokerage");
    assert_eq!(v["investment"]["cash_mode"], "internal");
    let back: AccountFields = serde_json::from_value(v).unwrap();
    assert_eq!(back, fields);

    let q: RegisterQuery = serde_json::from_value(json!({
        "account": 1, "date_from": "2026-01-01", "date_to": null, "payee": null,
        "category": null, "tag": null, "cleared": "unmarked", "text": "costco",
        "sort": "check_num", "descending": true, "limit": 100, "offset": 0
    }))
    .unwrap();
    assert_eq!(q.limit, Some(100));
    assert_eq!(q.text.as_deref(), Some("costco"));
    assert!(q.descending);
    assert_eq!(
        serde_json::to_value(Counterpart::Transfer(kansha_core::accounts::AccountId(4))).unwrap(),
        json!({ "kind": "transfer", "id": 4 })
    );
    assert_eq!(
        serde_json::to_value(Counterpart::Split).unwrap(),
        json!({ "kind": "split" })
    );
}

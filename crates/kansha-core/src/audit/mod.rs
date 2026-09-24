//! Audit history for display (AUD-020). Entries are written by the
//! repositories (`persistence::audit`); this module turns their
//! before/after JSON into a field-by-field list of what changed, so the UI
//! shows changes without parsing JSON or comparing values itself.

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::date::Timestamp;
use crate::error::Result;
pub use crate::persistence::audit::{AuditAction, AuditEntity, AuditRecord};

/// One field that differs between the before and after snapshots.
/// Nested values use paths like `postings[1].amount`. `None` means the
/// field did not exist on that side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FieldChange {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// An audit entry with its changes spelled out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AuditEntry {
    pub id: i64,
    pub at: Timestamp,
    pub action: AuditAction,
    /// `ui`, `import`, `scheduler`, or `system`.
    pub origin: String,
    pub import_batch_id: Option<i64>,
    /// A create lists every field (`before` empty); a delete lists every
    /// field (`after` empty); anything else lists only differing fields.
    pub changes: Vec<FieldChange>,
}

/// History of one record, oldest first.
pub fn history(conn: &Connection, entity: AuditEntity, entity_id: i64) -> Result<Vec<AuditEntry>> {
    crate::persistence::audit::history(conn, entity, entity_id)?
        .iter()
        .map(describe)
        .collect()
}

/// Spell out what one entry changed.
pub fn describe(record: &AuditRecord) -> Result<AuditEntry> {
    let before = flatten_json(record.before_json.as_deref())?;
    let after = flatten_json(record.after_json.as_deref())?;
    let mut changes = Vec::new();
    for (path, b) in &before {
        let a = after.iter().find(|(p, _)| p == path).map(|(_, v)| v);
        if a != Some(b) {
            changes.push(FieldChange {
                path: path.clone(),
                before: Some(b.clone()),
                after: a.cloned(),
            });
        }
    }
    for (path, a) in &after {
        if !before.iter().any(|(p, _)| p == path) {
            changes.push(FieldChange {
                path: path.clone(),
                before: None,
                after: Some(a.clone()),
            });
        }
    }
    // A create or delete reports every field, changed by definition.
    Ok(AuditEntry {
        id: record.id,
        at: record.at,
        action: record.action,
        origin: record.origin.clone(),
        import_batch_id: record.import_batch_id,
        changes,
    })
}

/// (path, text) leaves of a JSON document, in document order.
fn flatten_json(json: Option<&str>) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    if let Some(text) = json {
        let value: Value = serde_json::from_str(text)?;
        walk(&value, String::new(), &mut out);
    }
    Ok(out)
}

fn walk(value: &Value, path: String, out: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                let child = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                walk(v, child, out);
            }
        }
        Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                walk(v, format!("{path}[{i}]"), out);
            }
        }
        Value::String(s) => out.push((path, s.clone())),
        other => out.push((path, other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(action: AuditAction, before: Option<&str>, after: Option<&str>) -> AuditRecord {
        AuditRecord {
            id: 1,
            at: "2026-06-30T12:00:00Z".parse().unwrap(),
            entity: AuditEntity::Txn,
            entity_id: 7,
            action,
            before_json: before.map(str::to_string),
            after_json: after.map(str::to_string),
            origin: "ui".into(),
            import_batch_id: None,
        }
    }

    fn change(path: &str, before: Option<&str>, after: Option<&str>) -> FieldChange {
        FieldChange {
            path: path.into(),
            before: before.map(str::to_string),
            after: after.map(str::to_string),
        }
    }

    #[test]
    fn update_lists_only_differing_fields_with_nested_paths() {
        let e = describe(&record(
            AuditAction::Update,
            Some(r#"{"memo":"a","postings":[{"amount":"-1.00"},{"amount":"1.00"}],"n":1}"#),
            Some(r#"{"memo":"b","postings":[{"amount":"-2.00"},{"amount":"1.00"}],"n":1}"#),
        ))
        .unwrap();
        assert_eq!(
            e.changes,
            vec![
                change("memo", Some("a"), Some("b")),
                change("postings[0].amount", Some("-1.00"), Some("-2.00")),
            ]
        );
    }

    #[test]
    fn create_and_delete_list_every_field_and_arrays_can_grow() {
        let e = describe(&record(
            AuditAction::Create,
            None,
            Some(r#"{"a":"x","b":null}"#),
        ))
        .unwrap();
        assert_eq!(
            e.changes,
            vec![
                change("a", None, Some("x")),
                change("b", None, Some("null"))
            ]
        );
        let e = describe(&record(AuditAction::Delete, Some(r#"{"a":"x"}"#), None)).unwrap();
        assert_eq!(e.changes, vec![change("a", Some("x"), None)]);
        let e = describe(&record(
            AuditAction::Update,
            Some(r#"{"l":["a"]}"#),
            Some(r#"{"l":["a","b"]}"#),
        ))
        .unwrap();
        assert_eq!(e.changes, vec![change("l[1]", None, Some("b"))]);
    }
}

//! The Standard categories list for a new book (UI-085, P-06): the data
//! file is well formed and every line can be created in a fresh book.

use std::collections::{HashMap, HashSet};

use kansha_core::Origin;
use kansha_core::categories::CategoryKind;
use kansha_core::persistence::{categories, reports};

use crate::fixture::{clock, db};

const LIST: &str = include_str!("../../data/standard-categories.toml");

struct Line {
    path: String,
    kind: CategoryKind,
    tax_related: bool,
    tax_line: Option<(String, String)>,
}

fn lines() -> Vec<Line> {
    let doc: toml::Table = LIST.parse().unwrap();
    let str_of = |t: &toml::Table, k: &str| t[k].as_str().unwrap().to_owned();
    doc["categories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let t = v.as_table().unwrap();
            Line {
                path: str_of(t, "path"),
                kind: str_of(t, "kind").parse().unwrap(),
                tax_related: t["tax_related"].as_bool().unwrap(),
                tax_line: t
                    .get("tax_line")
                    .map(|l| l.as_table().unwrap())
                    .map(|l| (str_of(l, "form"), str_of(l, "line"))),
            }
        })
        .collect()
}

#[test]
fn the_list_is_well_formed() {
    let db = db();
    let tax: HashSet<(String, String)> = reports::tax_lines(db.conn())
        .unwrap()
        .into_iter()
        .map(|t| (t.form, t.line))
        .collect();
    let builtin: HashSet<String> = categories::list(db.conn())
        .unwrap()
        .into_iter()
        .filter(|c| c.system.is_some())
        .map(|c| c.fields.name.to_lowercase())
        .collect();
    let mut kinds: HashMap<String, CategoryKind> = HashMap::new();
    for l in lines() {
        assert!(
            !builtin.contains(&l.path.to_lowercase()),
            "{} is built in",
            l.path
        );
        assert_ne!(l.kind, CategoryKind::Equity, "{}", l.path);
        if let Some((parent, _)) = l.path.rsplit_once(':') {
            assert_eq!(
                kinds.get(parent),
                Some(&l.kind),
                "{}: parent missing, later, or of another kind",
                l.path
            );
        }
        if let Some(t) = &l.tax_line {
            assert!(tax.contains(t), "{}: no tax line {t:?}", l.path);
            assert!(l.tax_related, "{}: a tax line but not tax-related", l.path);
        }
        let dup = kinds.insert(l.path.clone(), l.kind);
        assert!(dup.is_none(), "{} twice", l.path);
        assert!(
            !kinds
                .keys()
                .any(|k| k != &l.path && k.eq_ignore_ascii_case(&l.path)),
            "{} twice (case)",
            l.path
        );
    }
}

#[test]
fn every_line_can_be_created_in_a_new_book() {
    let mut db = db();
    let clock = clock();
    let tax: HashMap<(String, String), _> = reports::tax_lines(db.conn())
        .unwrap()
        .into_iter()
        .map(|t| ((t.form, t.line), t.id))
        .collect();
    let list = lines();
    db.write(&clock, Origin::System, |tx| {
        for l in &list {
            let made = categories::create_path(tx, &l.path, l.kind)?;
            let mut f = made.fields.clone();
            f.tax_related = l.tax_related;
            f.tax_line = l.tax_line.as_ref().map(|t| tax[t]);
            categories::update(tx, made.id, &f)?;
        }
        Ok(())
    })
    .unwrap();
    let all = categories::list(db.conn()).unwrap();
    assert_eq!(
        all.iter().filter(|c| c.system.is_none()).count(),
        list.len()
    );
}

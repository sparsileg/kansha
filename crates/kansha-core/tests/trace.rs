//! Traceability (TEST-070): every [1.0] engine requirement in the TXN,
//! INT, REC, RCN, INV, LOT, POS, and RPT areas is cited by at least one
//! test or scenario. Run with `just trace`; it lists the IDs no test
//! cites and fails if there are any.
//!
//! A citation is the ID anywhere in `crates/kansha-core/tests/`, in a
//! scenario file under `tests/scenarios/`, or in the `#[cfg(test)]`
//! part of a `crates/kansha-core/src/` file.

// Test helpers panic on setup failure by design.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

const AREAS: &[&str] = &["TXN", "INT", "REC", "RCN", "INV", "LOT", "POS", "RPT"];

/// Requirements with nothing to test, and why.
const EXEMPT: &[(&str, &str)] = &[(
    "INT-050",
    "no derived caches exist; every figure is computed from the ledger",
)];

/// IDs tagged [1.0] in the spec's requirement lines: `**TXN-010** [1.0]`.
fn required(spec: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for part in spec.split("**").collect::<Vec<_>>().windows(2) {
        let (id, after) = (part[0], part[1]);
        let Some((area, num)) = id.split_once('-') else {
            continue;
        };
        if AREAS.contains(&area)
            && !num.is_empty()
            && num.chars().all(|c| c.is_ascii_digit())
            && after.starts_with(" [1.0]")
        {
            ids.push(id.to_string());
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

fn files(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            files(&p, ext, out);
        } else if p.extension().is_some_and(|x| x == ext) {
            out.push(p);
        }
    }
}

/// `id` as a whole token: `TXN-010` does not match `TXN-0100`.
fn cites(text: &str, id: &str) -> bool {
    text.match_indices(id).any(|(i, _)| {
        !text[i + id.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
    })
}

#[test]
#[ignore = "run with `just trace`"]
fn every_engine_requirement_is_cited_by_a_test() {
    let core = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = core.join("../..");
    let spec = fs::read_to_string(root.join("devdocs/kansha-spec.md")).unwrap();
    let ids = required(&spec);
    assert!(ids.len() > 50, "found only {} IDs; spec format?", ids.len());

    let mut corpus = String::new();
    let mut paths = Vec::new();
    files(&core.join("tests"), "rs", &mut paths);
    files(&root.join("tests/scenarios"), "toml", &mut paths);
    for p in &paths {
        corpus.push_str(&fs::read_to_string(p).unwrap());
    }
    let mut src = Vec::new();
    files(&core.join("src"), "rs", &mut src);
    for p in &src {
        let text = fs::read_to_string(p).unwrap();
        if let Some(i) = text.find("#[cfg(test)]") {
            corpus.push_str(&text[i..]);
        }
    }

    let exempt = |id: &str| EXEMPT.iter().any(|(e, _)| *e == id);
    let missing: Vec<&String> = ids
        .iter()
        .filter(|id| !exempt(id) && !cites(&corpus, id))
        .collect();
    println!(
        "{} of {} engine requirements cited by a test",
        ids.len() - missing.len() - EXEMPT.len(),
        ids.len()
    );
    for (id, why) in EXEMPT {
        println!("  exempt: {id} ({why})");
    }
    for id in &missing {
        println!("  not cited: {id}");
    }
    assert!(
        missing.is_empty(),
        "{} requirement(s) not cited",
        missing.len()
    );
}

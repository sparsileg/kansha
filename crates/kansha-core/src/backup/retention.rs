//! Backup retention (BAK-040): keep the newest `keep_last` automatic
//! backups of a book, plus the newest one of each of the last
//! `keep_months` calendar months (UTC). Manual backups are never deleted,
//! nor any file whose name is not a backup of this book (another book's
//! backups in the same folder are its own business).
//!
//! Timeout backups (SET-050) are temporary and outside that count: at
//! most one is kept, the newest, and only while no backup of another
//! kind (manual ones included) is newer.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::{BackupKind, parse_parts};
use crate::date::Timestamp;
use crate::error::Result;

/// `YYYY-MM` of a timestamp as a month count.
fn month_index(t: Timestamp) -> Option<i64> {
    let s = t.to_string();
    let y: i64 = s.get(0..4)?.parse().ok()?;
    let m: i64 = s.get(5..7)?.parse().ok()?;
    Some(y * 12 + m - 1)
}

/// Which of `names` to delete for `book`. Pure; [`prune`] applies it.
pub fn prune_plan(
    book: &str,
    names: &[String],
    now: Timestamp,
    keep_last: u32,
    keep_months: u32,
) -> Vec<String> {
    let mut auto: Vec<(Timestamp, u32, &String)> = Vec::new();
    let mut timeouts: Vec<(Timestamp, u32, &String)> = Vec::new();
    // The newest backup that is not a timeout, of any kind.
    let mut newest_other: Option<(Timestamp, u32)> = None;
    for n in names {
        let Some((t, kind, seq)) = parse_parts(book, n) else {
            continue;
        };
        if kind == BackupKind::Timeout {
            timeouts.push((t, seq, n));
            continue;
        }
        newest_other = newest_other.max(Some((t, seq)));
        if kind != BackupKind::Manual {
            auto.push((t, seq, n));
        }
    }
    // Newest first: by time, then by the `-n` suffix of a later backup in
    // the same second; the name makes the plan deterministic.
    auto.sort_by(|a, b| b.cmp(a));

    let mut keep: BTreeSet<&String> = auto
        .iter()
        .take(usize::try_from(keep_last).unwrap_or(usize::MAX))
        .map(|(_, _, n)| *n)
        .collect();
    if let Some(this_month) = month_index(now) {
        let first = this_month - i64::from(keep_months) + 1;
        let mut months_seen = BTreeSet::new();
        for (t, _, n) in &auto {
            let Some(m) = month_index(*t) else { continue };
            if keep_months > 0 && m >= first && m <= this_month && months_seen.insert(m) {
                keep.insert(n);
            }
        }
    }
    let mut delete: Vec<String> = auto
        .iter()
        .filter(|(_, _, n)| !keep.contains(n))
        .map(|(_, _, n)| (*n).clone())
        .collect();

    // A timeout backup stays only if it is the newest timeout and nothing
    // else is newer.
    timeouts.sort_by(|a, b| b.cmp(a));
    for (i, (t, seq, n)) in timeouts.iter().enumerate() {
        let superseded = i > 0 || newest_other.is_some_and(|o| o > (*t, *seq));
        if superseded {
            delete.push((*n).clone());
        }
    }
    delete
}

/// Delete `book`'s automatic backups in `folder` beyond the retention
/// rule. Returns the files deleted; one that cannot be deleted is skipped.
pub fn prune(
    folder: &Path,
    book: &str,
    now: Timestamp,
    keep_last: u32,
    keep_months: u32,
) -> Result<Vec<PathBuf>> {
    let mut names = Vec::new();
    for e in std::fs::read_dir(folder)? {
        let e = e?;
        if !e.file_type()?.is_file() {
            continue;
        }
        if let Some(n) = e.file_name().to_str() {
            names.push(n.to_owned());
        }
    }
    let mut deleted = Vec::new();
    for n in prune_plan(book, &names, now, keep_last, keep_months) {
        let p = folder.join(&n);
        if std::fs::remove_file(&p).is_ok() {
            deleted.push(p);
        }
    }
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::file_name;

    fn t(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    fn name(s: &str, kind: BackupKind) -> String {
        file_name("kansha", t(s), kind)
    }

    #[test]
    fn keeps_last_n_and_one_per_month() {
        let names = vec![
            name("2026-09-29T10:00:00Z", BackupKind::Close),
            name("2026-09-28T10:00:00Z", BackupKind::Close),
            name("2026-09-27T10:00:00Z", BackupKind::Close),
            name("2026-08-31T10:00:00Z", BackupKind::Close), // newest of Aug
            name("2026-08-01T10:00:00Z", BackupKind::Bulk),
            name("2026-07-15T10:00:00Z", BackupKind::Close), // newest of Jul
            name("2025-09-30T10:00:00Z", BackupKind::Close), // 13 months back
            name("2025-01-05T10:00:00Z", BackupKind::Manual), // manual: kept
            "notes.txt".to_owned(),
            "kansha-backup-garbage.zip".to_owned(),
        ];
        let del = prune_plan("kansha", &names, t("2026-09-29T12:00:00Z"), 2, 3);
        assert_eq!(
            del,
            vec![
                name("2026-09-27T10:00:00Z", BackupKind::Close),
                name("2026-08-01T10:00:00Z", BackupKind::Bulk),
                name("2025-09-30T10:00:00Z", BackupKind::Close),
            ]
        );
        // September 2025 is the 13th month back.
        let del12 = prune_plan("kansha", &names, t("2026-09-29T12:00:00Z"), 2, 12);
        assert!(del12.contains(&name("2025-09-30T10:00:00Z", BackupKind::Close)));
        let del13 = prune_plan("kansha", &names, t("2026-09-29T12:00:00Z"), 2, 13);
        assert!(!del13.contains(&name("2025-09-30T10:00:00Z", BackupKind::Close)));
    }

    #[test]
    fn a_timeout_backup_stays_only_while_it_is_the_newest_of_all() {
        let now = t("2026-09-29T12:00:00Z");
        let close = name("2026-09-29T10:00:00Z", BackupKind::Close);
        let timeout = name("2026-09-29T11:00:00Z", BackupKind::Timeout);
        // Newer than the close backup: kept.
        assert!(prune_plan("kansha", &[close.clone(), timeout.clone()], now, 10, 12).is_empty());
        // A close backup after it: deleted.
        let later_close = name("2026-09-29T11:30:00Z", BackupKind::Close);
        assert_eq!(
            prune_plan(
                "kansha",
                &[close, timeout.clone(), later_close],
                now,
                10,
                12
            ),
            vec![timeout.clone()]
        );
        // So is a manual one, though a manual backup is never itself deleted.
        let manual = name("2026-09-29T11:30:00Z", BackupKind::Manual);
        assert_eq!(
            prune_plan("kansha", &[timeout.clone(), manual], now, 10, 12),
            vec![timeout]
        );
    }

    #[test]
    fn only_the_newest_timeout_backup_is_kept() {
        let now = t("2026-09-29T12:00:00Z");
        let a = name("2026-09-29T10:00:00Z", BackupKind::Timeout);
        let b = name("2026-09-29T10:05:00Z", BackupKind::Timeout);
        let c = name("2026-09-29T10:10:00Z", BackupKind::Timeout);
        let del = prune_plan("kansha", &[a.clone(), b.clone(), c.clone()], now, 10, 12);
        assert_eq!(del.len(), 2);
        assert!(del.contains(&a) && del.contains(&b) && !del.contains(&c));
    }

    #[test]
    fn timeout_backups_do_not_use_up_the_keep_count() {
        let now = t("2026-09-29T12:00:00Z");
        let old = name("2026-09-28T10:00:00Z", BackupKind::Close);
        let timeout = name("2026-09-29T11:00:00Z", BackupKind::Timeout);
        // keep_last = 1: the close backup stays; the timeout is not counted
        // against it and is the newest, so it stays too.
        assert!(prune_plan("kansha", &[old, timeout], now, 1, 0).is_empty());
    }

    #[test]
    fn a_timeout_in_the_old_name_style_is_treated_the_same() {
        let now = t("2026-09-29T12:00:00Z");
        let timeout = "kansha-backup-2026-09-29T11-00-00Z-timeout.zip".to_owned();
        let close = name("2026-09-29T11:30:00Z", BackupKind::Close);
        assert_eq!(
            prune_plan("kansha", &[timeout.clone(), close], now, 10, 12),
            vec![timeout]
        );
    }

    #[test]
    fn old_and_new_name_styles_prune_by_the_same_rules() {
        let now = t("2026-09-29T12:00:00Z");
        let old_a = "kansha-backup-2026-09-27T10-00-00Z-close.zip".to_owned();
        let old_manual = "kansha-backup-2025-01-05T10-00-00Z-manual.zip".to_owned();
        let new_b = name("2026-09-29T10:00:00Z", BackupKind::Close);
        let del = prune_plan("kansha", &[old_a.clone(), old_manual, new_b], now, 1, 0);
        assert_eq!(del, vec![old_a]);
    }

    #[test]
    fn keep_last_covers_everything_when_large() {
        let names = vec![
            name("2026-09-29T10:00:00Z", BackupKind::Close),
            name("2020-01-01T10:00:00Z", BackupKind::Close),
        ];
        assert!(prune_plan("kansha", &names, t("2026-09-29T12:00:00Z"), 10, 0).is_empty());
        assert_eq!(
            prune_plan("kansha", &names, t("2026-09-29T12:00:00Z"), 1, 0),
            vec![name("2020-01-01T10:00:00Z", BackupKind::Close)]
        );
    }

    #[test]
    fn a_later_backup_in_the_same_second_is_newer() {
        let first = name("2026-09-29T10:00:00Z", BackupKind::Close);
        let second = first.replace(".zip", "-2.zip");
        let third = first.replace(".zip", "-10.zip");
        let names = vec![first.clone(), second.clone(), third.clone()];
        assert_eq!(
            prune_plan("kansha", &names, t("2026-09-29T12:00:00Z"), 1, 0),
            vec![second, first]
        );
    }

    #[test]
    fn prune_deletes_only_planned_files() {
        let dir = tempfile::tempdir().unwrap();
        let keep = name("2026-09-29T10:00:00Z", BackupKind::Close);
        let old = name("2026-01-29T10:00:00Z", BackupKind::Close);
        let manual = name("2020-01-29T10:00:00Z", BackupKind::Manual);
        for n in [&keep, &old, &manual, &"other.zip".to_owned()] {
            std::fs::write(dir.path().join(n), b"x").unwrap();
        }
        let del = prune(dir.path(), "kansha", t("2026-09-29T12:00:00Z"), 1, 1).unwrap();
        assert_eq!(del, vec![dir.path().join(&old)]);
        assert!(dir.path().join(&keep).exists());
        assert!(dir.path().join(&manual).exists());
        assert!(dir.path().join("other.zip").exists());
    }

    #[test]
    fn a_book_never_prunes_another_books_backups() {
        // Two books back up to one folder (finding #2 of the Phase 8
        // review): each keeps its own count and its own timeout.
        let now = t("2026-09-29T12:00:00Z");
        let b = |book: &str, s: &str, kind| file_name(book, t(s), kind);
        let names = vec![
            b("barton2026", "2026-09-29T08:00:00Z", BackupKind::Close),
            b("barton2026", "2026-09-29T09:00:00Z", BackupKind::Timeout),
            b("carol", "2026-09-29T10:00:00Z", BackupKind::Close),
            b("carol", "2026-09-29T11:00:00Z", BackupKind::Close),
        ];
        assert!(prune_plan("barton2026", &names, now, 1, 0).is_empty());
        assert_eq!(
            prune_plan("carol", &names, now, 1, 0),
            vec![b("carol", "2026-09-29T10:00:00Z", BackupKind::Close)]
        );
    }
}

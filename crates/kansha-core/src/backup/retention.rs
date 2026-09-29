//! Backup retention (BAK-040): keep the newest `keep_last` automatic
//! backups, plus the newest one of each of the last `keep_months`
//! calendar months (UTC). Manual backups are never deleted, nor any file
//! whose name is not a Kansha backup's.

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

/// Which of `names` to delete. Pure; [`prune`] applies it.
pub fn prune_plan(
    names: &[String],
    now: Timestamp,
    keep_last: u32,
    keep_months: u32,
) -> Vec<String> {
    let mut auto: Vec<(Timestamp, u32, &String)> = names
        .iter()
        .filter_map(|n| {
            let (t, kind, seq) = parse_parts(n)?;
            (kind != BackupKind::Manual).then_some((t, seq, n))
        })
        .collect();
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
    auto.iter()
        .filter(|(_, _, n)| !keep.contains(n))
        .map(|(_, _, n)| (*n).clone())
        .collect()
}

/// Delete automatic backups in `folder` beyond the retention rule.
/// Returns the files deleted; one that cannot be deleted is skipped.
pub fn prune(
    folder: &Path,
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
    for n in prune_plan(&names, now, keep_last, keep_months) {
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
        file_name(t(s), kind)
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
        let del = prune_plan(&names, t("2026-09-29T12:00:00Z"), 2, 3);
        assert_eq!(
            del,
            vec![
                name("2026-09-27T10:00:00Z", BackupKind::Close),
                name("2026-08-01T10:00:00Z", BackupKind::Bulk),
                name("2025-09-30T10:00:00Z", BackupKind::Close),
            ]
        );
        // September 2025 is the 13th month back.
        let del12 = prune_plan(&names, t("2026-09-29T12:00:00Z"), 2, 12);
        assert!(del12.contains(&name("2025-09-30T10:00:00Z", BackupKind::Close)));
        let del13 = prune_plan(&names, t("2026-09-29T12:00:00Z"), 2, 13);
        assert!(!del13.contains(&name("2025-09-30T10:00:00Z", BackupKind::Close)));
    }

    #[test]
    fn keep_last_covers_everything_when_large() {
        let names = vec![
            name("2026-09-29T10:00:00Z", BackupKind::Close),
            name("2020-01-01T10:00:00Z", BackupKind::Close),
        ];
        assert!(prune_plan(&names, t("2026-09-29T12:00:00Z"), 10, 0).is_empty());
        assert_eq!(
            prune_plan(&names, t("2026-09-29T12:00:00Z"), 1, 0),
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
            prune_plan(&names, t("2026-09-29T12:00:00Z"), 1, 0),
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
        let del = prune(dir.path(), t("2026-09-29T12:00:00Z"), 1, 1).unwrap();
        assert_eq!(del, vec![dir.path().join(&old)]);
        assert!(dir.path().join(&keep).exists());
        assert!(dir.path().join(&manual).exists());
        assert!(dir.path().join("other.zip").exists());
    }
}

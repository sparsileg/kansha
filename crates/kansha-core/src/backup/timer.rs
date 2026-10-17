//! When a timed backup is due (SET-050): a fixed delay after the first
//! change since the last backup. The clock runs from the first change, not
//! the last, so steady editing still gets a backup; any backup (of any
//! kind) resets it, since there is nothing new to save. A backup made
//! in steps saves only what was there when its image was taken: changes
//! after that wait for the next one.
//!
//! State only; the caller supplies the time and does the backup.

use crate::date::Timestamp;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BackupTimer {
    first_change: Option<Timestamp>,
    /// While a backup's image is taken and the backup not yet done: the
    /// first change since the image (`Some(None)` when none yet).
    since_image: Option<Option<Timestamp>>,
}

impl BackupTimer {
    /// A change was saved. Only the first since the last backup counts.
    pub fn changed(&mut self, now: Timestamp) {
        self.first_change.get_or_insert(now);
        if let Some(since) = self.since_image.as_mut() {
            since.get_or_insert(now);
        }
    }

    /// A backup's image was taken; the backup is not done yet.
    pub fn taken(&mut self) {
        self.since_image = Some(None);
    }

    /// A backup was made: only changes after its image wait.
    pub fn backed_up(&mut self) {
        self.first_change = self.since_image.take().flatten();
    }

    /// A backup whose image was taken failed: everything still waits.
    pub fn failed(&mut self) {
        self.since_image = None;
    }

    /// A change is waiting for a backup.
    pub fn pending(&self) -> bool {
        self.first_change.is_some()
    }

    /// The delay is up. `minutes == 0` (or less) means timed backups are
    /// off.
    pub fn due(&self, now: Timestamp, minutes: i64) -> bool {
        minutes > 0
            && self
                .first_change
                .is_some_and(|t| now.seconds_since(t) >= minutes.saturating_mul(60))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    #[test]
    fn nothing_changed_is_never_due() {
        let timer = BackupTimer::default();
        assert!(!timer.pending());
        assert!(!timer.due(t("2030-01-01T00:00:00Z"), 5));
    }

    #[test]
    fn due_after_the_delay_from_the_first_change() {
        let mut timer = BackupTimer::default();
        timer.changed(t("2026-09-29T10:00:00Z"));
        assert!(timer.pending());
        assert!(!timer.due(t("2026-09-29T10:04:59Z"), 5));
        assert!(timer.due(t("2026-09-29T10:05:00Z"), 5));
    }

    #[test]
    fn later_changes_do_not_move_the_clock() {
        let mut timer = BackupTimer::default();
        timer.changed(t("2026-09-29T10:00:00Z"));
        timer.changed(t("2026-09-29T10:04:00Z"));
        assert!(timer.due(t("2026-09-29T10:05:00Z"), 5));
    }

    #[test]
    fn a_backup_of_any_kind_resets_it() {
        let mut timer = BackupTimer::default();
        timer.changed(t("2026-09-29T10:00:00Z"));
        timer.backed_up();
        assert!(!timer.pending());
        assert!(!timer.due(t("2026-09-29T11:00:00Z"), 5));
        // The next change starts a new delay.
        timer.changed(t("2026-09-29T11:00:00Z"));
        assert!(!timer.due(t("2026-09-29T11:04:00Z"), 5));
        assert!(timer.due(t("2026-09-29T11:05:00Z"), 5));
    }

    #[test]
    fn zero_minutes_turns_it_off() {
        let mut timer = BackupTimer::default();
        timer.changed(t("2026-09-29T10:00:00Z"));
        assert!(!timer.due(t("2030-01-01T00:00:00Z"), 0));
        assert!(!timer.due(t("2030-01-01T00:00:00Z"), -1));
    }

    #[test]
    fn a_clock_that_runs_backwards_is_not_due() {
        let mut timer = BackupTimer::default();
        timer.changed(t("2026-09-29T10:00:00Z"));
        assert!(!timer.due(t("2026-09-29T09:00:00Z"), 5));
    }

    #[test]
    fn a_change_after_the_image_was_taken_stays_pending() {
        let mut timer = BackupTimer::default();
        timer.changed(t("2026-09-29T10:00:00Z"));
        timer.taken();
        timer.changed(t("2026-09-29T10:02:00Z"));
        timer.changed(t("2026-09-29T10:03:00Z"));
        timer.backed_up();
        // Not in that backup: its delay runs from the first such change.
        assert!(timer.pending());
        assert!(!timer.due(t("2026-09-29T10:06:59Z"), 5));
        assert!(timer.due(t("2026-09-29T10:07:00Z"), 5));
        // The next backup takes it.
        timer.taken();
        timer.backed_up();
        assert!(!timer.pending());
    }

    #[test]
    fn a_failed_backup_leaves_the_change_waiting() {
        let mut timer = BackupTimer::default();
        timer.changed(t("2026-09-29T10:00:00Z"));
        timer.taken();
        timer.changed(t("2026-09-29T10:02:00Z"));
        timer.failed();
        assert!(timer.due(t("2026-09-29T10:05:00Z"), 5));
        // A backup in one step takes everything.
        timer.backed_up();
        assert!(!timer.pending());
    }
}

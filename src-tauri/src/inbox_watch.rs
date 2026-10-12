//! Watch the open book's inbox (`<book>-inbox/`) so entries from the
//! command line show while the window sits untouched (CLI-040). The
//! window reads the inbox when told; this only says when to.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

use notify::event::{AccessKind, EventKind};
use notify::{Event, RecursiveMode, Watcher};

/// How long the inbox must be quiet before the window is told: a file
/// copied in by hand arrives in several writes.
const QUIET: Duration = Duration::from_millis(500);

enum Msg {
    Fs(notify::Result<Event>),
    Stop,
}

/// A running watch; dropping it stops the watch.
pub struct InboxWatch {
    folder: PathBuf,
    stop: Sender<Msg>,
}

impl InboxWatch {
    /// Watch `folder`, calling `changed` once a file arrives in it and the
    /// folder has been quiet for a moment. The folder need not exist yet:
    /// its parent is watched until it is made. `None` when neither can be
    /// watched; the window still reads the inbox when it gets focus.
    pub fn start(folder: PathBuf, changed: impl Fn() + Send + 'static) -> Option<InboxWatch> {
        let (tx, rx) = mpsc::channel();
        let fs_tx = tx.clone();
        let mut watcher = notify::recommended_watcher(move |r| {
            let _ = fs_tx.send(Msg::Fs(r));
        })
        .ok()?;
        let parent = folder.parent().map(Path::to_path_buf);
        let by_parent = parent
            .as_deref()
            .is_some_and(|p| watcher.watch(p, RecursiveMode::NonRecursive).is_ok());
        let mut watching = watcher.watch(&folder, RecursiveMode::NonRecursive).is_ok();
        if !by_parent && !watching {
            return None;
        }
        let inbox = folder.clone();
        std::thread::spawn(move || {
            loop {
                let mut due = match rx.recv() {
                    Ok(Msg::Fs(Ok(e))) => arrived(&e, &inbox),
                    Ok(Msg::Fs(Err(_))) => false,
                    Ok(Msg::Stop) | Err(_) => return,
                };
                if !watching && due {
                    // The inbox was just made: watch it from now on.
                    watching = watcher.watch(&inbox, RecursiveMode::NonRecursive).is_ok();
                }
                if !due {
                    continue;
                }
                // Wait for the inbox to go quiet, then tell the window once.
                while due {
                    match rx.recv_timeout(QUIET) {
                        Ok(Msg::Fs(_)) => {}
                        Err(RecvTimeoutError::Timeout) => due = false,
                        Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
                changed();
            }
        });
        Some(InboxWatch { folder, stop: tx })
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }
}

impl Drop for InboxWatch {
    fn drop(&mut self) {
        let _ = self.stop.send(Msg::Stop);
    }
}

/// Whether `e` may bring an entry: a file showing up in the inbox (not a
/// hidden temporary one, not a removal or a read), or the inbox itself
/// being made. Changes to the book beside it do not count.
fn arrived(e: &Event, inbox: &Path) -> bool {
    if matches!(
        e.kind,
        EventKind::Remove(_) | EventKind::Access(AccessKind::Read | AccessKind::Open(_))
    ) {
        return false;
    }
    e.paths.iter().any(|p| {
        p == inbox
            || (p.parent() == Some(inbox)
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| !n.starts_with('.')))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};

    fn event(kind: EventKind, path: PathBuf) -> Event {
        Event::new(kind).add_path(path)
    }

    #[test]
    fn only_files_arriving_in_the_inbox_count() {
        let inbox = PathBuf::from("/books/test-inbox");
        let to = EventKind::Modify(ModifyKind::Name(RenameMode::To));
        assert!(arrived(&event(to, inbox.join("a.txt")), &inbox));
        assert!(arrived(
            &event(EventKind::Create(CreateKind::Folder), inbox.clone()),
            &inbox
        ));
        // The hidden temporary file `kansha add` writes first.
        assert!(!arrived(
            &event(EventKind::Create(CreateKind::File), inbox.join(".a.txt")),
            &inbox
        ));
        // The app removing a file it received.
        assert!(!arrived(
            &event(EventKind::Remove(RemoveKind::File), inbox.join("a.txt")),
            &inbox
        ));
        // The book itself, beside the inbox.
        assert!(!arrived(
            &event(
                EventKind::Modify(ModifyKind::Any),
                PathBuf::from("/books/test.db")
            ),
            &inbox
        ));
    }

    /// Wait up to 5 s for a call.
    fn called(rx: &mpsc::Receiver<()>) -> bool {
        rx.recv_timeout(Duration::from_secs(5)).is_ok()
    }

    #[test]
    fn a_file_renamed_into_the_inbox_is_reported_once() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = dir.path().join("test-inbox");
        std::fs::create_dir(&inbox).unwrap();
        let (tx, rx) = mpsc::channel();
        let _w = InboxWatch::start(inbox.clone(), move || tx.send(()).unwrap()).unwrap();
        std::fs::write(inbox.join(".a.txt"), "kansha-entries 1\n").unwrap();
        std::fs::rename(inbox.join(".a.txt"), inbox.join("a.txt")).unwrap();
        assert!(called(&rx));
        assert!(rx.recv_timeout(QUIET * 3).is_err(), "told twice");
    }

    #[test]
    fn an_inbox_made_after_the_watch_starts_is_watched() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = dir.path().join("test-inbox");
        let (tx, rx) = mpsc::channel();
        let _w = InboxWatch::start(inbox.clone(), move || tx.send(()).unwrap()).unwrap();
        std::fs::create_dir(&inbox).unwrap();
        assert!(called(&rx));
        std::fs::write(inbox.join("b.txt"), "kansha-entries 1\n").unwrap();
        assert!(called(&rx));
    }

    #[test]
    fn writes_beside_the_inbox_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = dir.path().join("test-inbox");
        std::fs::create_dir(&inbox).unwrap();
        let (tx, rx) = mpsc::channel();
        let _w = InboxWatch::start(inbox, move || tx.send(()).unwrap()).unwrap();
        std::fs::write(dir.path().join("test.db"), "x").unwrap();
        assert!(rx.recv_timeout(QUIET * 3).is_err());
    }

    #[test]
    fn dropping_the_watch_stops_it() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = dir.path().join("test-inbox");
        std::fs::create_dir(&inbox).unwrap();
        let (tx, rx) = mpsc::channel();
        let w = InboxWatch::start(inbox.clone(), move || {
            let _ = tx.send(());
        })
        .unwrap();
        drop(w);
        std::thread::sleep(Duration::from_millis(100));
        std::fs::write(inbox.join("c.txt"), "kansha-entries 1\n").unwrap();
        assert!(rx.recv_timeout(QUIET * 3).is_err());
    }
}

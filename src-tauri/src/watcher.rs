//! Watches the music folders while BitNot is running, so files that are
//! added, changed, moved, or removed show up without a manual rescan.
//! The watcher only observes; the scan that follows only reads.

use std::path::Path;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::scanner::is_audio_path;

pub struct FolderWatcher {
    _inner: RecommendedWatcher,
    /// How many folders are actually being watched.
    pub watching: usize,
}

fn is_relevant(event: &notify::Event) -> bool {
    let changes_files = matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    );
    // Audio files, or folders (a folder rename or delete carries no extension).
    changes_files
        && event
            .paths
            .iter()
            .any(|path| is_audio_path(path) || path.extension().is_none())
}

impl FolderWatcher {
    /// Starts watching. `on_change` is called (possibly many times in a burst)
    /// when something relevant changes; the caller debounces.
    /// Returns `None` when the operating system refuses to watch at all.
    pub fn start(
        folders: &[String],
        recursive: bool,
        on_change: impl Fn() + Send + 'static,
    ) -> Option<Self> {
        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                if result.as_ref().is_ok_and(is_relevant) {
                    on_change();
                }
            })
            .ok()?;
        let mode = if recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        let mut watching = 0;
        for folder in folders {
            // A folder that is offline right now is simply not watched.
            if watcher.watch(Path::new(folder), mode).is_ok() {
                watching += 1;
            }
        }
        Some(FolderWatcher {
            _inner: watcher,
            watching,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn watch(dir: &Path, recursive: bool) -> (FolderWatcher, mpsc::Receiver<()>) {
        let (sender, receiver) = mpsc::channel();
        let watcher = FolderWatcher::start(
            &[dir.to_string_lossy().into_owned()],
            recursive,
            move || {
                let _ = sender.send(());
            },
        )
        .expect("watcher starts");
        (watcher, receiver)
    }

    #[test]
    fn reports_added_and_removed_audio_files_but_not_other_files() {
        let tmp = tempfile::tempdir().unwrap();
        let (watcher, events) = watch(tmp.path(), false);
        assert_eq!(watcher.watching, 1);

        std::fs::write(tmp.path().join("notes.txt"), "hello").unwrap();
        assert!(
            events.recv_timeout(Duration::from_millis(700)).is_err(),
            "text files are ignored"
        );

        std::fs::write(tmp.path().join("song.mp3"), "x").unwrap();
        assert!(events.recv_timeout(Duration::from_secs(10)).is_ok());
        while events.recv_timeout(Duration::from_millis(500)).is_ok() {}

        std::fs::remove_file(tmp.path().join("song.mp3")).unwrap();
        assert!(events.recv_timeout(Duration::from_secs(10)).is_ok());
    }

    #[test]
    fn subfolders_are_watched_only_when_recursive() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("Album")).unwrap();
        let (_watcher, events) = watch(tmp.path(), true);
        std::fs::write(tmp.path().join("Album").join("deep.flac"), "x").unwrap();
        assert!(events.recv_timeout(Duration::from_secs(10)).is_ok());
    }

    #[test]
    fn an_unreachable_folder_is_skipped_without_failing() {
        let tmp = tempfile::tempdir().unwrap();
        let gone = tmp.path().join("not-here").to_string_lossy().into_owned();
        let here = tmp.path().to_string_lossy().into_owned();
        let watcher = FolderWatcher::start(&[gone, here], false, || {}).unwrap();
        assert_eq!(watcher.watching, 1);
    }
}

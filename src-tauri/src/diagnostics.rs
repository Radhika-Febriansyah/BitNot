//! Local diagnostics: a daily log file under AppData, a panic hook, and a
//! session marker that reveals whether the previous run ended cleanly.
//!
//! Logs stay on this computer. Nothing here uploads anything. Callers must not
//! log secrets, tokens, or the contents of the user's library.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use time::OffsetDateTime;

use crate::error::{AppError, Result};

const PREFIX: &str = "bitnot-";
const SUFFIX: &str = ".log";
const KEEP_DAYS: i64 = 14;
const MAX_READ_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LogFile {
    pub name: String,
    pub size_bytes: u64,
}

fn date_stamp(at: OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        at.year(),
        u8::from(at.month()),
        at.day()
    )
}

fn file_name_for(at: OffsetDateTime) -> String {
    format!("{PREFIX}{}{SUFFIX}", date_stamp(at))
}

/// Accepts only names this module produces: `bitnot-YYYY-MM-DD.log`.
fn is_log_name(name: &str) -> bool {
    let Some(date) = name
        .strip_prefix(PREFIX)
        .and_then(|rest| rest.strip_suffix(SUFFIX))
    else {
        return false;
    };
    date.len() == 10
        && date.chars().enumerate().all(|(i, c)| match i {
            4 | 7 => c == '-',
            _ => c.is_ascii_digit(),
        })
}

pub struct FileLogger {
    dir: PathBuf,
    current: Mutex<Option<(String, File)>>,
}

impl FileLogger {
    pub fn new(dir: PathBuf) -> Self {
        FileLogger {
            dir,
            current: Mutex::new(None),
        }
    }

    fn write_line(&self, line: &str) {
        let now = OffsetDateTime::now_utc();
        let name = file_name_for(now);
        let Ok(mut guard) = self.current.lock() else {
            return;
        };
        if guard.as_ref().map(|(n, _)| n != &name).unwrap_or(true) {
            let opened = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.dir.join(&name));
            match opened {
                Ok(file) => *guard = Some((name, file)),
                Err(_) => return,
            }
        }
        if let Some((_, file)) = guard.as_mut() {
            let _ = writeln!(
                file,
                "{}T{:02}:{:02}:{:02}Z {}",
                date_stamp(now),
                now.hour(),
                now.minute(),
                now.second(),
                line
            );
        }
    }

    /// Closes the open log file so it can be deleted.
    fn release(&self) {
        if let Ok(mut guard) = self.current.lock() {
            *guard = None;
        }
    }
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            // Only this crate's records: dependencies may log paths or payloads.
            // BITNOT_LOG_ALL=1 also records dependencies, for debugging only.
            if record.target().starts_with("bit_not")
                || std::env::var_os("BITNOT_LOG_ALL").is_some()
            {
                self.write_line(&format!("{:<5} {}", record.level(), record.args()));
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut guard) = self.current.lock() {
            if let Some((_, file)) = guard.as_mut() {
                let _ = file.flush();
            }
        }
    }
}

/// Installs the file logger and a panic hook. Safe to call once per process.
pub fn install(dir: &Path, session_marker: Option<PathBuf>) -> Option<&'static FileLogger> {
    fs::create_dir_all(dir).ok()?;
    let logger: &'static FileLogger = Box::leak(Box::new(FileLogger::new(dir.to_path_buf())));
    log::set_logger(logger).ok()?;
    log::set_max_level(log::LevelFilter::Info);

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown location".into());
        logger.write_line(&format!("PANIC at {location}"));
        if let Some(marker) = &session_marker {
            // A panic is a real crash in any build.
            let _ = fs::write(marker, MARKER_PANICKED);
        }
        previous(info);
    }));
    Some(logger)
}

pub fn list(dir: &Path) -> Result<Vec<LogFile>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_log_name(&name) && entry.file_type()?.is_file() {
            files.push(LogFile {
                name,
                size_bytes: entry.metadata()?.len(),
            });
        }
    }
    files.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(files)
}

/// Returns the last part of a log file (up to 256 KB) as text.
pub fn read(dir: &Path, name: &str) -> Result<String> {
    if !is_log_name(name) {
        return Err(AppError::invalid("That is not a BitNot log file."));
    }
    let mut file = File::open(dir.join(name))
        .map_err(|_| AppError::NotFound("That log file no longer exists.".into()))?;
    let len = file.metadata()?.len();
    if len > MAX_READ_BYTES {
        file.seek(SeekFrom::Start(len - MAX_READ_BYTES))?;
    }
    let mut bytes = Vec::new();
    file.take(MAX_READ_BYTES).read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Deletes every diagnostic log. Returns how many files were removed.
pub fn clear(dir: &Path, logger: Option<&FileLogger>) -> Result<usize> {
    if let Some(logger) = logger {
        logger.release();
    }
    let mut removed = 0;
    for file in list(dir)? {
        fs::remove_file(dir.join(&file.name))?;
        removed += 1;
    }
    Ok(removed)
}

/// Removes logs older than the retention window. Failures are ignored.
pub fn prune_old(dir: &Path, now: OffsetDateTime) {
    let cutoff = file_name_for(now - time::Duration::days(KEEP_DAYS));
    if let Ok(files) = list(dir) {
        for file in files {
            if file.name < cutoff {
                let _ = fs::remove_file(dir.join(&file.name));
            }
        }
    }
}

const MARKER_RUNNING: &str = "running";
const MARKER_RUNNING_DEV: &str = "running:dev";
const MARKER_PANICKED: &str = "panicked";

/// Marks the start of a session. Returns `true` when the previous session did
/// not shut down cleanly (its marker is still present).
///
/// A development build is routinely killed by the build tool when it restarts
/// the app, which is not a crash. So a marker left by a development session
/// counts only if that session panicked. Release builds are unaffected: any
/// leftover marker is reported.
pub fn begin_session(marker: &Path, dev_build: bool) -> Result<bool> {
    let previous = fs::read_to_string(marker).ok();
    fs::write(
        marker,
        if dev_build {
            MARKER_RUNNING_DEV
        } else {
            MARKER_RUNNING
        },
    )?;
    Ok(match previous.as_deref() {
        None => false,
        Some(MARKER_RUNNING_DEV) => false,
        Some(_) => true,
    })
}

/// Marks a clean shutdown.
pub fn end_session(marker: &Path) {
    let _ = fs::remove_file(marker);
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::Log;

    #[test]
    fn log_name_validation_blocks_traversal() {
        assert!(is_log_name("bitnot-2026-11-14.log"));
        assert!(!is_log_name("bitnot-2026-11-14.log.bak"));
        assert!(!is_log_name("..\\bitnot-2026-11-14.log"));
        assert!(!is_log_name("bitnot-../../x.log"));
        assert!(!is_log_name("app.db"));
        assert!(!is_log_name(""));
    }

    #[test]
    fn writes_lists_reads_and_clears() {
        let tmp = tempfile::tempdir().unwrap();
        let logger = FileLogger::new(tmp.path().to_path_buf());
        logger.write_line("INFO  hello diagnostics");
        logger.flush();
        fs::write(tmp.path().join("unrelated.txt"), "keep me").unwrap();

        let files = list(tmp.path()).unwrap();
        assert_eq!(files.len(), 1);
        let text = read(tmp.path(), &files[0].name).unwrap();
        assert!(text.contains("hello diagnostics"));

        assert!(read(tmp.path(), "unrelated.txt").is_err());
        assert!(read(tmp.path(), "bitnot-1999-01-01.log").is_err());

        assert_eq!(clear(tmp.path(), Some(&logger)).unwrap(), 1);
        assert!(list(tmp.path()).unwrap().is_empty());
        assert!(tmp.path().join("unrelated.txt").exists());

        // Logging keeps working after a clear.
        logger.write_line("INFO  again");
        assert_eq!(list(tmp.path()).unwrap().len(), 1);
    }

    #[test]
    fn read_returns_only_the_tail_of_large_files() {
        let tmp = tempfile::tempdir().unwrap();
        let name = "bitnot-2026-01-01.log";
        let mut body = "a".repeat(MAX_READ_BYTES as usize);
        body.push_str("THE-END");
        fs::write(tmp.path().join(name), &body).unwrap();
        let text = read(tmp.path(), name).unwrap();
        assert_eq!(text.len() as u64, MAX_READ_BYTES);
        assert!(text.ends_with("THE-END"));
    }

    #[test]
    fn prune_keeps_recent_logs() {
        let tmp = tempfile::tempdir().unwrap();
        let now = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
        let old = file_name_for(now - time::Duration::days(KEEP_DAYS + 1));
        let recent = file_name_for(now - time::Duration::days(1));
        fs::write(tmp.path().join(&old), "x").unwrap();
        fs::write(tmp.path().join(&recent), "x").unwrap();
        prune_old(tmp.path(), now);
        let names: Vec<String> = list(tmp.path())
            .unwrap()
            .into_iter()
            .map(|f| f.name)
            .collect();
        assert_eq!(names, vec![recent]);
    }

    #[test]
    fn session_marker_detects_unclean_shutdown() {
        let tmp = tempfile::tempdir().unwrap();
        let marker = tmp.path().join("session.lock");
        assert!(!begin_session(&marker, false).unwrap());
        // No clean shutdown happened, so the next start sees the marker.
        assert!(begin_session(&marker, false).unwrap());
        end_session(&marker);
        assert!(!begin_session(&marker, false).unwrap());
    }

    #[test]
    fn a_killed_development_session_is_not_a_crash_but_a_panic_is() {
        let tmp = tempfile::tempdir().unwrap();
        let marker = tmp.path().join("session.lock");
        // Development build restarted by the build tool: no clean shutdown.
        assert!(!begin_session(&marker, true).unwrap());
        assert!(
            !begin_session(&marker, true).unwrap(),
            "not reported as a crash"
        );
        // The same session panicking is a real crash, in a dev build too.
        fs::write(&marker, MARKER_PANICKED).unwrap();
        assert!(begin_session(&marker, true).unwrap());
        // A release session that vanished is always reported, whoever starts next.
        end_session(&marker);
        assert!(!begin_session(&marker, false).unwrap());
        assert!(begin_session(&marker, true).unwrap());
    }
}

//! Crash reports and the record of running sessions.
//!
//! Each running copy of the app keeps a small record in the `Sessions`
//! folder of the application data folder, named by its process id, with the
//! autosave backup of a job that has unsaved changes. A clean quit removes
//! it. A record left behind by a process that is gone is a session that
//! crashed: the next start offers the crash report written then and the
//! backup the record names ([`Session::take_recovery`]).
//!
//! A panic anywhere in the shell or the kernel writes a report to the
//! `Crash Reports` folder ([`install_panic_hook`]). Nothing leaves the Mac.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Folder of the crash reports, in the application data folder.
pub const REPORTS_FOLDER: &str = "Crash Reports";
/// Folder of the session records, in the application data folder.
pub const SESSIONS_FOLDER: &str = "Sessions";
/// How many crash reports are kept; older ones are removed.
pub const KEEP_REPORTS: usize = 20;
/// The ending of an autosave backup file (`io::native::backup_path`).
pub const BACKUP_SUFFIX: &str = ".backup.carve";

/// What a running session records about itself.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    /// Process id of the session.
    pub pid: u32,
    /// When it started, seconds since 1970.
    pub started: u64,
    /// The autosave backup of the job while it has unsaved changes.
    pub backup: Option<String>,
}

/// What the page offers at start up after a crash.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recovery {
    /// A session before this one ended without quitting.
    pub crashed: bool,
    /// The crash report it wrote, if it wrote one.
    pub report: Option<String>,
    /// Backups of unsaved jobs it left, that still exist.
    pub backups: Vec<String>,
    /// For each backup, the file it becomes when the user opens it later
    /// ([`kept_path`]), where autosave never writes.
    pub kept: Vec<String>,
}

/// This session's record and what the ones before it left behind.
#[derive(Debug)]
pub struct Session {
    root: PathBuf,
    own: Mutex<Record>,
    left: Mutex<Option<Vec<(PathBuf, Record)>>>,
}

impl Session {
    /// Starts this session's record in `root` (the application data
    /// folder) and collects the records of sessions whose process `alive`
    /// says is gone. Failing to write the record only loses the offer after
    /// a crash, so errors are printed, not returned.
    pub fn begin(root: &Path, pid: u32, now: u64, alive: impl Fn(u32) -> bool) -> Session {
        let folder = root.join(SESSIONS_FOLDER);
        let mut left = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&folder) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let record = std::fs::read(&path)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Record>(&bytes).ok());
                match record {
                    Some(record) if record.pid == pid || alive(record.pid) => {}
                    Some(record) => left.push((path, record)),
                    None => {
                        std::fs::remove_file(&path).ok();
                    }
                }
            }
        }
        left.sort_by_key(|(_, record)| std::cmp::Reverse(record.started));
        let session = Session {
            root: root.to_path_buf(),
            own: Mutex::new(Record {
                pid,
                started: now,
                backup: None,
            }),
            left: Mutex::new(Some(left)),
        };
        session.write();
        session
    }

    fn own_path(&self, pid: u32) -> PathBuf {
        self.root.join(SESSIONS_FOLDER).join(format!("{pid}.json"))
    }

    fn write(&self) {
        let Ok(record) = self.own.lock().map(|r| r.clone()) else {
            return;
        };
        let path = self.own_path(record.pid);
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| {
                let partial = path.with_extension("json.partial");
                let text = serde_json::to_vec(&record).map_err(std::io::Error::other)?;
                std::fs::write(&partial, text)?;
                std::fs::rename(&partial, &path)
            });
        if let Err(error) = written {
            eprintln!(
                "the session record {} could not be written: {error}",
                path.display()
            );
        }
    }

    /// Records the backup of the job's unsaved changes, or that there is
    /// none worth restoring (the job was saved, closed or replaced).
    pub fn record_backup(&self, backup: Option<String>) {
        let changed = match self.own.lock() {
            Ok(mut own) if own.backup != backup => {
                own.backup = backup;
                true
            }
            _ => false,
        };
        if changed {
            self.write();
        }
    }

    /// Removes this session's record: the app quit normally.
    pub fn end(&self) {
        if let Ok(own) = self.own.lock() {
            std::fs::remove_file(self.own_path(own.pid)).ok();
        }
    }

    /// What the sessions that crashed left, once: the newest crash report
    /// written since the first of them started, and their backups that
    /// still exist. Their records are removed.
    pub fn take_recovery(&self) -> Recovery {
        let left = self
            .left
            .lock()
            .ok()
            .and_then(|mut left| left.take())
            .unwrap_or_default();
        let Some(since) = left.iter().map(|(_, record)| record.started).min() else {
            return Recovery::default();
        };
        let mut backups: Vec<String> = Vec::new();
        for (path, record) in &left {
            if let Some(backup) = &record.backup {
                if Path::new(backup).is_file() && !backups.contains(backup) {
                    backups.push(backup.clone());
                }
            }
            std::fs::remove_file(path).ok();
        }
        let report = reports(&self.root.join(REPORTS_FOLDER))
            .into_iter()
            .rev()
            .find(|(written, _)| *written >= since)
            .map(|(_, path)| path.display().to_string());
        let kept = backups
            .iter()
            .map(|backup| {
                kept_path(Path::new(backup)).map_or_else(String::new, |p| p.display().to_string())
            })
            .collect();
        Recovery {
            crashed: true,
            report,
            backups,
            kept,
        }
    }
}

/// The reports in `folder`, oldest first, with the second each was written.
fn reports(folder: &Path) -> Vec<(u64, PathBuf)> {
    let mut found: Vec<(u64, PathBuf)> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("txt"))
        .filter_map(|path| {
            let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok()?;
            let seconds = modified.duration_since(UNIX_EPOCH).ok()?.as_secs();
            Some((seconds, path))
        })
        .collect();
    found.sort();
    found
}

/// Seconds since 1970, now.
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The date and time of `seconds` since 1970, in UTC, as
/// `(year, month, day, hour, minute, second)`.
pub fn civil(seconds: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    // Howard Hinnant's days-to-civil for the proleptic Gregorian calendar.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        year,
        month,
        day,
        (rest / 3_600) as u32,
        (rest % 3_600 / 60) as u32,
        (rest % 60) as u32,
    )
}

/// The text of a crash report.
pub fn report_text(
    version: &str,
    seconds: u64,
    thread: &str,
    message: &str,
    location: &str,
    backtrace: &str,
) -> String {
    let (y, mo, d, h, mi, s) = civil(seconds);
    format!(
        "CarveStudio crash report\n\
         Version: {version}\n\
         Time: {y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02} UTC\n\
         System: {} {}\n\
         Thread: {thread}\n\
         Message: {message}\n\
         Location: {location}\n\
         \n\
         Stack:\n{backtrace}\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

/// Writes `text` as a new report in `folder` named by its time, and keeps
/// only the newest [`KEEP_REPORTS`]. Answers the report's path.
///
/// # Errors
/// When the folder cannot be made or the report cannot be written.
pub fn write_report(folder: &Path, seconds: u64, text: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(folder)?;
    let (y, mo, d, h, mi, s) = civil(seconds);
    let stem = format!("CarveStudio {y:04}-{mo:02}-{d:02} {h:02}.{mi:02}.{s:02}");
    let mut path = folder.join(format!("{stem}.txt"));
    let mut n = 2;
    while path.exists() {
        path = folder.join(format!("{stem} {n}.txt"));
        n += 1;
    }
    std::fs::write(&path, text)?;
    let all = reports(folder);
    if all.len() > KEEP_REPORTS {
        for (_, old) in &all[..all.len() - KEEP_REPORTS] {
            if old != &path {
                std::fs::remove_file(old).ok();
            }
        }
    }
    Ok(path)
}

/// The message of a panic payload.
pub fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "a panic without a message".to_string()
    }
}

/// Writes a crash report to `folder` for every panic, then lets the panic
/// go on as before (printed, and unwound or caught).
pub fn install_panic_hook(folder: PathBuf, version: String) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_default();
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        let text = report_text(
            &version,
            now(),
            thread.name().unwrap_or("unnamed"),
            &panic_message(info.payload()),
            &location,
            &backtrace,
        );
        match write_report(&folder, now(), &text) {
            Ok(path) => eprintln!("crash report written to {}", path.display()),
            Err(error) => eprintln!("the crash report could not be written: {error}"),
        }
        previous(info);
    }));
}

/// Whether process `pid` is running.
#[cfg(unix)]
pub fn process_alive(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // Signal 0 checks the process exists without touching it; EPERM means
    // it exists but belongs to someone else.
    // SAFETY: kill with signal 0 sends nothing and has no memory effects.
    let result = unsafe { libc::kill(pid, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Whether process `pid` is running; unknown elsewhere, so taken as yes
/// (no crash is ever reported).
#[cfg(not(unix))]
pub fn process_alive(_pid: u32) -> bool {
    true
}

/// Whether `path` names an autosave backup, the only files the page may
/// ask the shell to delete.
pub fn is_backup(path: &Path) -> bool {
    path.is_absolute()
        && path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(BACKUP_SUFFIX) && n.len() > BACKUP_SUFFIX.len())
}

/// Where a backup the user keeps for later goes: `sign recovered.carve`
/// beside `sign.backup.carve` (then `sign recovered 2.carve` and so on),
/// a name autosave never writes, so the next backup of a job cannot
/// overwrite it. None for a path that is not a backup.
pub fn kept_path(backup: &Path) -> Option<PathBuf> {
    if !is_backup(backup) {
        return None;
    }
    let name = backup.file_name()?.to_str()?;
    let stem = &name[..name.len() - BACKUP_SUFFIX.len()];
    let folder = backup.parent()?;
    let mut n = 1;
    loop {
        let candidate = if n == 1 {
            folder.join(format!("{stem} recovered.carve"))
        } else {
            folder.join(format!("{stem} recovered {n}.carve"))
        };
        if !candidate.exists() {
            return Some(candidate);
        }
        n += 1;
    }
}

/// Moves a backup the user keeps for later to [`kept_path`] and answers
/// where it went.
///
/// # Errors
/// When `backup` is not a backup file or cannot be renamed.
pub fn keep_backup(backup: &Path) -> std::io::Result<PathBuf> {
    let kept = kept_path(backup).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} is not a backup", backup.display()),
        )
    })?;
    std::fs::rename(backup, &kept)?;
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("carve-session-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("temp folder");
        dir
    }

    #[test]
    fn civil_dates_match_the_calendar() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        // 2000-02-29 12:34:56 UTC, a leap day in a century leap year.
        assert_eq!(civil(951_827_696), (2000, 2, 29, 12, 34, 56));
        // 2026-09-28 00:00:00 UTC.
        assert_eq!(civil(1_790_553_600), (2026, 9, 28, 0, 0, 0));
        // The last second of 2024-12-31.
        assert_eq!(civil(1_735_689_599), (2024, 12, 31, 23, 59, 59));
    }

    #[test]
    fn a_clean_quit_leaves_nothing_to_recover() {
        let root = folder("clean");
        let first = Session::begin(&root, 101, 1_000, |_| false);
        first.record_backup(Some("/a/job.backup.carve".into()));
        first.end();
        let second = Session::begin(&root, 102, 2_000, |_| false);
        assert_eq!(second.take_recovery(), Recovery::default());
    }

    #[test]
    fn a_crashed_session_offers_its_backup_and_report_once() {
        let root = folder("crashed");
        let backup = root.join("job.backup.carve");
        std::fs::write(&backup, b"zip").expect("backup");
        let crashed = Session::begin(&root, 201, now() - 5, |_| false);
        crashed.record_backup(Some(backup.display().to_string()));
        let report = write_report(&root.join(REPORTS_FOLDER), now(), "boom").expect("report");
        // The process 201 is gone; it never removed its record.
        let next = Session::begin(&root, 202, now(), |_| false);
        let recovery = next.take_recovery();
        assert_eq!(
            recovery,
            Recovery {
                crashed: true,
                report: Some(report.display().to_string()),
                backups: vec![backup.display().to_string()],
                kept: vec![root.join("job recovered.carve").display().to_string()],
            }
        );
        assert_eq!(next.take_recovery(), Recovery::default(), "offered once");
        next.end();
        let after = Session::begin(&root, 203, now(), |_| false);
        assert!(!after.take_recovery().crashed, "the old record is gone");
    }

    #[test]
    fn a_saved_job_or_a_running_copy_is_not_offered() {
        let root = folder("saved");
        let backup = root.join("job.backup.carve");
        std::fs::write(&backup, b"zip").expect("backup");
        let saved = Session::begin(&root, 301, 10, |_| false);
        saved.record_backup(Some(backup.display().to_string()));
        saved.record_backup(None);
        let running = Session::begin(&root, 302, 20, |_| false);
        running.record_backup(Some(backup.display().to_string()));
        let next = Session::begin(&root, 303, 30, |pid| pid == 302);
        let recovery = next.take_recovery();
        assert!(recovery.crashed, "301 ended without quitting");
        assert!(
            recovery.backups.is_empty(),
            "its job had been saved: {recovery:?}"
        );
        assert!(recovery.report.is_none(), "no report since 301 started");
    }

    #[test]
    fn a_missing_backup_or_an_old_report_is_not_offered() {
        let root = folder("missing");
        write_report(&root.join(REPORTS_FOLDER), now(), "older crash").expect("report");
        let crashed = Session::begin(&root, 401, now() + 60, |_| false);
        crashed.record_backup(Some(root.join("gone.backup.carve").display().to_string()));
        let next = Session::begin(&root, 402, now() + 120, |_| false);
        assert_eq!(
            next.take_recovery(),
            Recovery {
                crashed: true,
                report: None,
                backups: Vec::new(),
                kept: Vec::new(),
            }
        );
    }

    #[test]
    fn reports_are_kept_to_the_newest_twenty() {
        let root = folder("reports");
        let folder = root.join(REPORTS_FOLDER);
        let mut last = PathBuf::new();
        for i in 0..(KEEP_REPORTS + 3) {
            last =
                write_report(&folder, 1_000 + i as u64, &format!("report {i}")).expect("written");
        }
        assert_eq!(reports(&folder).len(), KEEP_REPORTS);
        assert!(last.is_file());
        assert_eq!(
            last.file_name().and_then(|n| n.to_str()),
            Some("CarveStudio 1970-01-01 00.17.02.txt")
        );
        // Two in one second get a number rather than overwrite each other.
        let twin = write_report(&folder, 1_022, "again").expect("written");
        assert_eq!(
            twin.file_name().and_then(|n| n.to_str()),
            Some("CarveStudio 1970-01-01 00.17.02 2.txt")
        );
    }

    #[test]
    fn a_report_names_the_version_time_and_place() {
        let text = report_text(
            "1.2.3",
            951_827_696,
            "main",
            "index out of range",
            "src/a.rs:4:9",
            "0: frame",
        );
        assert!(
            text.starts_with(
                "CarveStudio crash report\nVersion: 1.2.3\nTime: 2000-02-29 12:34:56 UTC\n"
            ),
            "{text}"
        );
        assert!(
            text.contains("\nThread: main\nMessage: index out of range\nLocation: src/a.rs:4:9\n"),
            "{text}"
        );
        assert!(text.ends_with("Stack:\n0: frame\n"), "{text}");
    }

    #[test]
    fn the_panic_hook_writes_a_report_and_lets_the_panic_go_on() {
        let root = folder("hook");
        let reports_folder = root.join(REPORTS_FOLDER);
        install_panic_hook(reports_folder.clone(), "9.8.7".to_string());
        let caught = std::panic::catch_unwind(|| panic!("the hook test panics"));
        // Back to the default hook for the rest of the tests.
        let _ = std::panic::take_hook();
        assert!(caught.is_err(), "the panic still unwinds");
        let written = reports(&reports_folder);
        assert_eq!(written.len(), 1);
        let text = std::fs::read_to_string(&written[0].1).expect("readable");
        assert!(
            text.starts_with("CarveStudio crash report\nVersion: 9.8.7\n"),
            "{text}"
        );
        assert!(text.contains("\nMessage: the hook test panics\n"), "{text}");
        assert!(
            text.contains("\nLocation: app/src-tauri/src/session.rs:"),
            "{text}"
        );
    }

    #[test]
    fn only_backups_may_be_discarded() {
        assert!(is_backup(Path::new("/Users/me/sign.backup.carve")));
        assert!(!is_backup(Path::new("/Users/me/sign.carve")));
        assert!(!is_backup(Path::new("/Users/me/.backup.carve")));
        assert!(!is_backup(Path::new("sign.backup.carve")), "relative");
    }

    #[test]
    fn a_kept_backup_moves_where_autosave_never_writes() {
        let root = folder("kept");
        let backup = root.join("Untitled.backup.carve");
        std::fs::write(&backup, b"first").expect("backup");
        let kept = keep_backup(&backup).expect("kept");
        assert_eq!(kept, root.join("Untitled recovered.carve"));
        assert!(!backup.exists(), "the backup name is free for autosave");
        std::fs::write(&backup, b"second").expect("next backup");
        assert_eq!(
            keep_backup(&backup).expect("kept again"),
            root.join("Untitled recovered 2.carve")
        );
        assert_eq!(std::fs::read(&kept).expect("first kept"), b"first");
        assert!(keep_backup(&root.join("Untitled recovered.carve")).is_err());
        assert_eq!(kept_path(Path::new("/j/sign.carve")), None);
    }

    #[test]
    fn this_process_is_alive() {
        assert!(process_alive(std::process::id()));
        assert!(!process_alive(u32::MAX));
    }
}

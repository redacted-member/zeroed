//! DWG decoding in a child process.
//!
//! DWG files are untrusted input for a large decoder. The reader checks
//! every size a file states against what the file could hold, but an
//! allocation that still fails aborts the whole process where no panic
//! guard can catch it, and a decode could run long. So the hosts decode
//! DWG files in a child process: their own executable started with
//! [`HELPER_FLAG`], which [`run_helper`] answers. The parent waits a
//! bounded time and turns a crash, an abort or a timeout into an ordinary
//! [`CoreError`].
//!
//! The child is given the file, the scale and the units hint as arguments
//! and writes one JSON object to stdout: `{"paths": [...], "warnings":
//! [...]}` on success (exit code 0) or `{"error": "..."}` on failure (exit
//! code 1).

use std::ffi::OsString;
use std::io::{Read as _, Write as _};
use std::path::{Path as FsPath, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::super::{ImportWarning, ImportedPath};
use crate::error::CoreError;
use crate::geom::Path;

/// The argument that makes a host executable decode one DWG file.
pub const HELPER_FLAG: &str = "--decode-dwg";

/// How long a DWG file may take to decode.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// How often the parent looks whether the child has finished.
const POLL: Duration = Duration::from_millis(10);
/// The message for a decode that crashed or was stopped.
const NOT_READ: &str = "that DWG file could not be read";

/// A program that decodes DWG files out of process.
#[derive(Debug, Clone, PartialEq)]
pub struct Helper {
    program: PathBuf,
    /// Arguments before [`HELPER_FLAG`].
    args: Vec<OsString>,
    timeout: Duration,
}

impl Helper {
    /// `program` started as `program --decode-dwg <file> <scale> <units>`.
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Helper {
            program: program.into(),
            args: Vec::new(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// The running executable as the helper: hosts that call
    /// [`run_helper`] first thing in `main` decode with themselves. `None`
    /// when the executable's path is not known.
    pub fn current_exe() -> Option<Self> {
        std::env::current_exe().ok().map(Helper::new)
    }

    /// The same helper with `args` passed before [`HELPER_FLAG`].
    #[must_use]
    pub fn with_args<I, A>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = A>,
        A: Into<OsString>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    /// The same helper with another time limit.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Decodes `path` in a child process.
    pub(super) fn decode(
        &self,
        path: &FsPath,
        scale: f64,
        units_hint: Option<&str>,
    ) -> Result<(Vec<ImportedPath>, Vec<ImportWarning>), CoreError> {
        let mut child = Command::new(&self.program)
            .args(&self.args)
            .arg(HELPER_FLAG)
            .arg(path)
            .arg(scale.to_string())
            .arg(units_hint.unwrap_or(""))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| CoreError::format("DWG", format!("the DWG reader did not start: {e}")))?;
        // The reply is read on its own thread, so a large one cannot fill
        // the pipe while this thread waits for the child to end.
        let reader = child.stdout.take().map(|mut stdout| {
            std::thread::spawn(move || {
                let mut out = Vec::new();
                stdout.read_to_end(&mut out).map(|_| out)
            })
        });
        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
            }
        };
        // After a timeout the reader is left behind: a process the helper
        // started may still hold the pipe open, and joining would wait for it.
        let Some(status) = status else {
            return Err(CoreError::format(
                "DWG",
                format!(
                    "{NOT_READ}: it took longer than {} s",
                    self.timeout.as_secs()
                ),
            ));
        };
        let out = reader
            .and_then(|r| r.join().ok())
            .and_then(Result::ok)
            .unwrap_or_default();
        match serde_json::from_slice::<Reply>(&out) {
            Ok(Reply {
                error: Some(message),
                ..
            }) => Err(CoreError::format("DWG", message)),
            Ok(reply) if status.success() => Ok((
                reply.paths.into_iter().map(ImportedPath::from).collect(),
                reply.warnings,
            )),
            _ => Err(CoreError::format("DWG", NOT_READ)),
        }
    }
}

static HELPER: Mutex<Option<Helper>> = Mutex::new(None);

/// Decode DWG files through `helper` from now on, or in this process with
/// `None` (the default).
pub fn set_helper(helper: Option<Helper>) {
    if let Ok(mut slot) = HELPER.lock() {
        *slot = helper;
    }
}

/// The helper [`set_helper`] set.
pub(super) fn current() -> Option<Helper> {
    HELPER.lock().ok().and_then(|slot| slot.clone())
}

/// Answers a helper call. Hosts call this first thing in `main` with
/// `std::env::args()`: when the arguments start with [`HELPER_FLAG`] it
/// decodes the file, writes the reply to stdout and answers the exit code
/// to leave with; otherwise it answers `None` and the host starts as usual.
pub fn run_helper<I>(args: I) -> Option<i32>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter().skip(1);
    if args.next().as_deref() != Some(HELPER_FLAG) {
        return None;
    }
    let (Some(file), Some(scale)) = (args.next(), args.next()) else {
        return Some(2);
    };
    let scale = scale.parse::<f64>().unwrap_or(1.0);
    let units = args.next().filter(|u| !u.is_empty());
    let reply = match super::import_with(FsPath::new(&file), scale, units.as_deref(), None) {
        Ok((paths, warnings)) => Reply {
            paths: paths.into_iter().map(WirePath::from).collect(),
            warnings,
            error: None,
        },
        Err(error) => Reply {
            paths: Vec::new(),
            warnings: Vec::new(),
            error: Some(match error {
                CoreError::Format { message, .. } => message,
                other => other.to_string(),
            }),
        },
    };
    let failed = reply.error.is_some();
    let written = serde_json::to_writer(std::io::stdout().lock(), &reply)
        .ok()
        .and_then(|()| std::io::stdout().flush().ok());
    Some(if failed || written.is_none() { 1 } else { 0 })
}

/// What the child writes.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Reply {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    paths: Vec<WirePath>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<ImportWarning>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// An [`ImportedPath`] as the child writes it.
#[derive(Debug, Serialize, Deserialize)]
struct WirePath {
    layer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    path: Path,
}

impl From<ImportedPath> for WirePath {
    fn from(p: ImportedPath) -> Self {
        WirePath {
            layer: p.layer,
            name: p.name,
            path: p.path,
        }
    }
}

impl From<WirePath> for ImportedPath {
    fn from(p: WirePath) -> Self {
        ImportedPath {
            layer: p.layer,
            name: p.name,
            path: p.path,
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    const GOOD: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/dwg/circle-and-line-2018.dwg"
    );

    /// `/bin/sh -c script`, with the helper arguments after it as `$0`,
    /// `$1` and so on.
    fn shell(script: &str) -> Helper {
        Helper::new("/bin/sh").with_args(["-c", script])
    }

    fn message(result: Result<(Vec<ImportedPath>, Vec<ImportWarning>), CoreError>) -> String {
        result.expect_err("an error").to_string()
    }

    #[test]
    fn a_helper_that_runs_too_long_is_stopped() {
        // A grandchild holds the pipe open after the helper is stopped.
        let helper = shell("sleep 5 & wait").with_timeout(Duration::from_millis(200));
        let started = Instant::now();
        let text = message(super::super::import_with(
            FsPath::new(GOOD),
            1.0,
            None,
            Some(&helper),
        ));
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(text.contains("could not be read"), "{text}");
    }

    #[test]
    fn a_helper_that_aborts_or_writes_nonsense_is_an_error() {
        for script in [
            "kill -ABRT $$",
            "echo not json",
            "exit 0",
            "echo '{}'; exit 3",
        ] {
            let text = message(super::super::import_with(
                FsPath::new(GOOD),
                1.0,
                None,
                Some(&shell(script)),
            ));
            assert!(text.contains("could not be read"), "{script}: {text}");
        }
        let text = message(super::super::import_with(
            FsPath::new(GOOD),
            1.0,
            None,
            Some(&Helper::new("/no/such/helper")),
        ));
        assert!(text.contains("did not start"), "{text}");
    }

    #[test]
    fn the_reply_and_the_arguments_travel_as_expected() {
        // The child sees the flag, the file, the scale and the units.
        let echo = shell(
            r#"[ "$0" = --decode-dwg ] && [ "$2" = 2.5 ] && [ "$3" = inch ] || exit 9
               echo '{"error":"made up"}'; exit 1"#,
        );
        let text = message(super::super::import_with(
            FsPath::new(GOOD),
            2.5,
            Some("inch"),
            Some(&echo),
        ));
        assert!(text.contains("made up"), "{text}");
        let reply =
            r#"{"paths":[{"layer":"0","path":{"subpaths":[]}}],"warnings":["textSkipped"]}"#;
        let ok = shell(&format!("echo '{reply}'"));
        let (paths, warnings) =
            super::super::import_with(FsPath::new(GOOD), 1.0, None, Some(&ok)).expect("reads");
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].layer, "0");
        assert_eq!(warnings, vec![ImportWarning::TextSkipped]);
    }

    #[test]
    fn run_helper_ignores_other_arguments() {
        assert_eq!(run_helper(["app".to_string()]), None);
        assert_eq!(run_helper(["app".to_string(), "--other".to_string()]), None);
        assert_eq!(
            run_helper(["app".to_string(), HELPER_FLAG.to_string()]),
            Some(2)
        );
    }
}

//! Local diagnostic log for troubleshooting playback, sign-in and Discord.
//!
//! Written to `%LOCALAPPDATA%\io.github.zygerr.rustifiedyoutube\logs\RustifiedYoutube.log`. The previous
//! log is kept as `RustifiedYoutube.old.log` and the file rotates once it
//! grows past [`MAX_BYTES`]. The log never leaves this PC. Credentials must
//! never be logged; as a safeguard, lines mentioning cookie or authentication
//! material are redacted before they are written.
use std::{
    fmt::Display,
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

const FILE_NAME: &str = "RustifiedYoutube.log";
const OLD_FILE_NAME: &str = "RustifiedYoutube.old.log";
const MAX_BYTES: u64 = 4 * 1024 * 1024;
const MAX_LINE: usize = 2000;
/// Markers of credential values: `SID=` also covers SAPISID/APISID/HSID/SSID
/// assignments, `__Secure-` the secure cookie names.
const SENSITIVE: [&str; 6] = [
    "SID=",
    "SIDCC=",
    "LOGIN_INFO=",
    "__Secure-",
    "Cookie:",
    "cookie:",
];

#[derive(Clone, Copy)]
pub enum Level {
    Info,
    Warn,
    Error,
}
impl Level {
    fn label(self) -> &'static str {
        match self {
            Self::Info => "INFO ",
            Self::Warn => "WARN ",
            Self::Error => "ERROR",
        }
    }
}

struct Logger {
    file: File,
    written: u64,
}

static LOGGER: OnceLock<Mutex<Option<Logger>>> = OnceLock::new();

pub fn directory() -> Option<PathBuf> {
    rustified_core::session::app_dir()
        .ok()
        .map(|dir| dir.join("logs"))
}

fn open(directory: &std::path::Path) -> std::io::Result<Logger> {
    fs::create_dir_all(directory)?;
    let path = directory.join(FILE_NAME);
    if fs::metadata(&path).is_ok_and(|meta| meta.len() > MAX_BYTES / 4) {
        let _ = fs::rename(&path, directory.join(OLD_FILE_NAME));
    }
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    let written = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    Ok(Logger { file, written })
}

/// Opens the log and records crashes. Safe to call more than once.
pub fn init() {
    let logger = directory().and_then(|dir| open(&dir).ok());
    if LOGGER.set(Mutex::new(logger)).is_err() {
        return;
    }
    write(
        Level::Info,
        "app",
        format_args!(
            "RustifiedYoutube {} started (Windows {}, args: {})",
            env!("CARGO_PKG_VERSION"),
            windows_version(),
            std::env::args().skip(1).collect::<Vec<_>>().join(" ")
        ),
    );
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        write(Level::Error, "panic", info);
        previous(info);
    }));
}

fn windows_version() -> String {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("cmd")
        .args(["/C", "ver"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

/// Makes one safe log line: single-line, bounded, and without credentials.
fn sanitize(message: &str) -> String {
    if SENSITIVE.iter().any(|marker| message.contains(marker)) {
        return "[redacted: message referenced credentials]".into();
    }
    let mut line: String = message
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if line.len() > MAX_LINE {
        let mut end = MAX_LINE;
        while !line.is_char_boundary(end) {
            end -= 1;
        }
        line.truncate(end);
        line.push('…');
    }
    line
}

pub fn write(level: Level, area: &str, message: impl Display) {
    let Some(lock) = LOGGER.get() else {
        return;
    };
    let line = format!(
        "{} {} [{area}] {}\r\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
        level.label(),
        sanitize(&message.to_string())
    );
    let Ok(mut guard) = lock.lock() else {
        return;
    };
    if guard
        .as_ref()
        .is_some_and(|logger| logger.written > MAX_BYTES)
        && let Some(dir) = directory()
    {
        *guard = None;
        let _ = fs::rename(dir.join(FILE_NAME), dir.join(OLD_FILE_NAME));
        *guard = open(&dir).ok();
    }
    if let Some(logger) = guard.as_mut()
        && logger.file.write_all(line.as_bytes()).is_ok()
    {
        logger.written += line.len() as u64;
    }
}

pub fn info(area: &str, message: impl Display) {
    write(Level::Info, area, message);
}
pub fn warn(area: &str, message: impl Display) {
    write(Level::Warn, area, message);
}
pub fn error(area: &str, message: impl Display) {
    write(Level::Error, area, message);
}

/// Opens the folder containing the log in Explorer.
pub fn open_folder() {
    if let Some(dir) = directory() {
        let _ = fs::create_dir_all(&dir);
        let _ = open::that_detached(dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_are_never_written_and_lines_stay_single_and_bounded() {
        for secret in [
            "cookies: SAPISID=abc; SID=def",
            "__Secure-3PAPISID=rotated",
            "Cookie: VISITOR=1",
            "HSID=x",
        ] {
            assert_eq!(
                sanitize(secret),
                "[redacted: message referenced credentials]"
            );
        }
        // Describing cookie handling without values stays readable.
        assert_eq!(
            sanitize("Cookie refresh incomplete; keeping saved session"),
            "Cookie refresh incomplete; keeping saved session"
        );
        assert_eq!(sanitize("line one\nline two"), "line one line two");
        let long = sanitize(&"🚀".repeat(1000));
        assert!(long.len() <= MAX_LINE + '…'.len_utf8());
        assert!(long.ends_with('…'));
        assert_eq!(
            sanitize("play requested gen=3 id=Fixture0001"),
            "play requested gen=3 id=Fixture0001"
        );
    }

    #[test]
    fn a_large_previous_log_is_rotated_on_open() {
        let dir = std::env::temp_dir().join(format!("rustified-logs-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(FILE_NAME),
            vec![b'x'; (MAX_BYTES / 4 + 1) as usize],
        )
        .unwrap();
        let logger = open(&dir).unwrap();
        assert_eq!(logger.written, 0);
        assert!(dir.join(OLD_FILE_NAME).exists());
        drop(logger);
        let _ = fs::remove_dir_all(&dir);
    }
}

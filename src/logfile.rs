//! Saving the receive stream to a file with a time-based name.
//!
//! The name pattern is expanded once, when logging starts, so a session writes
//! to one predictable file even if the clock later crosses a minute or a day.
//! The tokens are deliberately simple and are listed in the dialog:
//!
//! | token | meaning | token | meaning |
//! |-------|---------|-------|---------|
//! | `%Y`  | 4-digit year | `%M` | month |
//! | `%y`  | 2-digit year | `%D` / `%d` | day |
//! | `%h` / `%H` | hour | `%m` | minute |
//! | `%s` / `%S` | second | `%f` | milliseconds |
//! | `%%`  | a literal `%` | | |
//!
//! Unknown tokens are left alone (`%q` stays `%q`) so a typo is visible in the
//! preview instead of silently disappearing.  The expanded name is stripped of
//! characters that are invalid in a file name on at least one supported
//! platform, so a pattern can never escape the chosen directory.

use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, Local, Timelike};

/// File-name pattern used until the user changes it.
pub const DEFAULT_PATTERN: &str = "xcom-%Y%M%D-%h%m%s.log";

/// Fallback when a pattern expands to nothing usable.
const FALLBACK_NAME: &str = "xcom.log";

/// Longest expanded name we try to create.
const MAX_NAME_CHARS: usize = 180;

/// Expand a name pattern with the time in `now`.
pub fn format_file_name(pattern: &str, now: DateTime<Local>) -> String {
    let mut out = String::with_capacity(pattern.len() + 16);
    let mut chars = pattern.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('Y') => out.push_str(&format!("{:04}", now.year())),
            Some('y') => out.push_str(&format!("{:02}", now.year().rem_euclid(100))),
            Some('M') => out.push_str(&format!("{:02}", now.month())),
            Some('D' | 'd') => out.push_str(&format!("{:02}", now.day())),
            Some('h' | 'H') => out.push_str(&format!("{:02}", now.hour())),
            Some('m') => out.push_str(&format!("{:02}", now.minute())),
            Some('s' | 'S') => out.push_str(&format!("{:02}", now.second())),
            Some('f') => out.push_str(&format!("{:03}", now.timestamp_subsec_millis())),
            Some('%') => out.push('%'),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            // A trailing `%` is kept as-is: the user is probably still typing.
            None => out.push('%'),
        }
    }
    out
}

/// Turn an expanded name into a safe, plain file name.
pub fn sanitize_file_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.trim().chars() {
        let invalid =
            ch.is_control() || matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|');
        out.push(if invalid { '-' } else { ch });
    }
    let out = out.trim_end_matches([' ', '.']).trim();
    let out: String = out.chars().take(MAX_NAME_CHARS).collect();
    if out.is_empty() || out == "." || out == ".." {
        FALLBACK_NAME.to_owned()
    } else {
        out
    }
}

/// Expand and sanitize `pattern` into the file name to create.
pub fn file_name_for(pattern: &str, now: DateTime<Local>) -> String {
    sanitize_file_name(&format_file_name(pattern, now))
}

/// Directory a log goes to when the setting is left empty.
pub fn default_directory() -> PathBuf {
    crate::settings::Settings::config_dir()
        .map(|dir| dir.join("logs"))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Resolve the configured directory, expanding a leading `~`.
///
/// An empty setting means the app's own `logs` directory under the config
/// directory, which is writable even when the program was launched from a
/// read-only working directory.
pub fn resolve_directory(directory: &str) -> PathBuf {
    let directory = directory.trim();
    if directory.is_empty() {
        return default_directory();
    }
    for prefix in ["~/", "~\\"] {
        if let Some(rest) = directory.strip_prefix(prefix) {
            let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
            if let Some(home) = home {
                return PathBuf::from(home).join(rest);
            }
        }
    }
    PathBuf::from(directory)
}

/// One append-only log file.
///
/// Writes are flushed per batch: a crashed or force-closed program must not
/// lose the last few kilobytes of a debugging session.
#[derive(Default)]
pub struct LogFile {
    writer: Option<BufWriter<File>>,
    path: Option<PathBuf>,
    bytes: u64,
}

impl LogFile {
    /// Whether a file is currently open for writing.
    pub fn is_active(&self) -> bool {
        self.writer.is_some()
    }

    /// Path of the open file, if any.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Bytes written since the current file was opened.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Create `directory` and open `name` for appending.
    ///
    /// An existing file with the same name is appended to rather than
    /// truncated: two sessions in the same second keep both halves.
    pub fn start(&mut self, directory: &Path, name: &str) -> io::Result<PathBuf> {
        self.stop();
        std::fs::create_dir_all(directory)?;
        let path = directory.join(name);
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        self.writer = Some(BufWriter::new(file));
        self.path = Some(path.clone());
        self.bytes = 0;
        Ok(path)
    }

    /// Append bytes and flush them to the operating system.
    pub fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        let Some(writer) = &mut self.writer else {
            return Ok(());
        };
        writer.write_all(bytes)?;
        writer.flush()?;
        self.bytes += bytes.len() as u64;
        Ok(())
    }

    /// Close the file, flushing anything still buffered.
    pub fn stop(&mut self) {
        if let Some(mut writer) = self.writer.take()
            && let Err(err) = writer.flush()
        {
            log::warn!("关闭日志文件失败: {err}");
        }
        self.path = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(year, month, day, hour, minute, second)
            .single()
            .expect("the test time must be unambiguous")
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "xcom-logfile-{tag}-{}-{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn expands_the_documented_tokens() {
        let now = at(2026, 10, 26, 14, 30, 5);
        assert_eq!(
            format_file_name("xcom-%Y%M%D-%h%m%s.log", now),
            "xcom-20261026-143005.log"
        );
        assert_eq!(
            format_file_name("%Y-%y-%M-%D_%d %h:%H %m %s:%S", now),
            "2026-26-10-26_26 14:14 30 05:05"
        );
        assert_eq!(format_file_name("ms-%f", now), "ms-000");
    }

    #[test]
    fn escapes_percent_and_keeps_unknown_tokens() {
        let now = at(2026, 10, 26, 14, 30, 5);
        assert_eq!(format_file_name("100%%-%q-%", now), "100%-%q-%");
    }

    #[test]
    fn sanitizes_path_separators_and_invalid_characters() {
        assert_eq!(
            sanitize_file_name("../a/b\\c:d*e?.log"),
            "..-a-b-c-d-e-.log"
        );
        assert_eq!(sanitize_file_name("trail. "), "trail");
        assert_eq!(sanitize_file_name("   "), FALLBACK_NAME);
        assert_eq!(sanitize_file_name("."), FALLBACK_NAME);
        assert_eq!(sanitize_file_name(".."), FALLBACK_NAME);
    }

    #[test]
    fn a_pattern_never_escapes_its_directory() {
        let now = at(2026, 10, 26, 14, 30, 5);
        let name = file_name_for("%Y/../../etc/passwd", now);
        assert!(!name.contains('/') && !name.contains('\\'), "{name}");
        assert_ne!(name, "..");
        assert_ne!(name, ".");
    }

    #[test]
    fn an_empty_directory_means_the_logs_directory() {
        let resolved = resolve_directory("   ");
        assert!(resolved.ends_with("logs"), "{}", resolved.display());
    }

    #[test]
    fn start_creates_the_directory_and_appends_writes() {
        let dir = temp_dir("append");
        let mut file = LogFile::default();
        let path = file.start(&dir, "session.log").unwrap();
        assert!(file.is_active());
        assert_eq!(file.path(), Some(path.as_path()));
        file.write_all(b"one").unwrap();
        assert_eq!(file.bytes(), 3);
        file.stop();
        assert!(!file.is_active());
        assert!(path.exists());

        // A second start with the same name appends instead of truncating.
        let path_again = file.start(&dir, "session.log").unwrap();
        assert_eq!(path_again, path);
        file.write_all(b"two").unwrap();
        file.stop();
        assert_eq!(std::fs::read(&path).unwrap(), b"onetwo");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn start_reports_a_directory_that_cannot_be_created() {
        let blocker = temp_dir("blocked");
        std::fs::create_dir_all(blocker.parent().unwrap()).ok();
        std::fs::write(&blocker, b"not a directory").unwrap();
        let mut file = LogFile::default();
        assert!(file.start(&blocker, "session.log").is_err());
        assert!(!file.is_active());
        std::fs::remove_file(&blocker).ok();
    }
}

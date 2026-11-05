//! Validation for values that reach external processes or the filesystem.

use crate::error::{AppError, AppResult};
use std::path::{Component, Path, PathBuf};

const MAX_SERIAL_LEN: usize = 64;

/// Serial numbers are passed to `adb -s` as a discrete argument, never through a shell, but they
/// are still restricted to the characters real devices use so a hostile value is rejected early.
pub fn validate_serial(serial: &str) -> AppResult<&str> {
    let ok = !serial.is_empty()
        && serial.len() <= MAX_SERIAL_LEN
        && !serial.starts_with('-')
        && serial
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'));
    if ok {
        Ok(serial)
    } else {
        Err(AppError::InvalidInput("Invalid device serial number.".into()))
    }
}

/// Serial numbers identify hardware, so logs keep only enough to tell devices apart.
pub fn redact(serial: &str) -> String {
    let prefix: String = serial.chars().take(3).collect();
    format!("{prefix}***")
}

/// Accept a bare file name only, and resolve it inside `dir`. Used for the media library so the
/// UI can never address a path outside the configured save directory.
pub fn file_in_dir(dir: &Path, name: &str) -> AppResult<PathBuf> {
    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(file)), None) if file == name => Ok(dir.join(file)),
        _ => Err(AppError::InvalidInput("Invalid file name.".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_real_serials() {
        for s in ["NKQL4D0162", "emulator-5554", "R58M12ABCDE", "0123456789ABCDEF"] {
            assert!(validate_serial(s).is_ok(), "{s}");
        }
    }

    #[test]
    fn rejects_injection_attempts() {
        for s in [
            "", "-s", "--help", "a b", "a;rm -rf /", "$(reboot)", "a|b", "a&b", "a\nb", "`x`",
            "a/b", "a\\b", "a'b", "a\"b",
        ] {
            assert!(validate_serial(s).is_err(), "{s:?}");
        }
        assert!(validate_serial(&"a".repeat(65)).is_err());
    }

    #[test]
    fn redacts_serial() {
        assert_eq!(redact("NKQL4D0162"), "NKQ***");
    }

    #[test]
    fn file_in_dir_blocks_traversal() {
        let dir = Path::new("/media");
        assert!(file_in_dir(dir, "a.png").is_ok());
        for n in ["../a.png", "..", ".", "a/b.png", "/etc/passwd", ""] {
            assert!(file_in_dir(dir, n).is_err(), "{n:?}");
        }
    }
}

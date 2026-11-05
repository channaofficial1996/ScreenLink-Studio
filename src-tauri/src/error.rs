use serde::{Serialize, Serializer};

/// Errors that cross the IPC boundary. `code` lets the UI pick recovery instructions; `message`
/// is safe to display (no serial numbers or file contents beyond what the user already sees).
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    InvalidInput(String),
    #[error("Android Debug Bridge (adb) could not be started: {0}")]
    AdbUnavailable(String),
    #[error("{0}")]
    Adb(String),
    #[error("Mirroring is not running.")]
    NotMirroring,
    #[error("A recording is already in progress.")]
    AlreadyRecording,
    #[error("{0}")]
    Mirror(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("File operation failed: {0}")]
    Io(#[from] std::io::Error),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::InvalidInput(_) => "invalid_input",
            AppError::AdbUnavailable(_) => "adb_unavailable",
            AppError::Adb(_) => "adb",
            AppError::NotMirroring => "not_mirroring",
            AppError::AlreadyRecording => "already_recording",
            AppError::Mirror(_) => "mirror",
            AppError::Unsupported(_) => "unsupported",
            AppError::Io(_) => "io",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;

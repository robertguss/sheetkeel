use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SheetkeelError {
    #[error("I/O error: {message}")]
    Io { message: String, fix_hint: String },

    #[error("Workbook error: {message}")]
    Workbook { message: String, fix_hint: String },

    #[error("Export/Pack error: {message}")]
    Export { message: String, fix_hint: String },

    #[error("Configuration error: {message}")]
    Config { message: String, fix_hint: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorEnvelope {
    pub ok: bool,
    pub code: &'static str,
    pub message: String,
    pub fix_hint: String,
    pub retryable: bool,
}

impl SheetkeelError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "E_IO",
            Self::Workbook { .. } => "E_WORKBOOK",
            Self::Export { .. } => "E_EXPORT",
            Self::Config { .. } => "E_CONFIG",
        }
    }

    pub fn fix_hint(&self) -> &str {
        match self {
            Self::Io { fix_hint, .. }
            | Self::Workbook { fix_hint, .. }
            | Self::Export { fix_hint, .. }
            | Self::Config { fix_hint, .. } => fix_hint,
        }
    }

    pub fn to_envelope(&self) -> ErrorEnvelope {
        ErrorEnvelope {
            ok: false,
            code: self.code(),
            message: self.to_string(),
            fix_hint: self.fix_hint().to_string(),
            retryable: false,
        }
    }

    pub fn io(err: impl std::fmt::Display, path: &str) -> Self {
        Self::Io {
            message: format!("{path}: {err}"),
            fix_hint: format!("Verify `{path}` exists and is readable/writable."),
        }
    }

    pub fn workbook(message: impl Into<String>, fix_hint: impl Into<String>) -> Self {
        Self::Workbook {
            message: message.into(),
            fix_hint: fix_hint.into(),
        }
    }

    pub fn export(message: impl Into<String>, fix_hint: impl Into<String>) -> Self {
        Self::Export {
            message: message.into(),
            fix_hint: fix_hint.into(),
        }
    }

    pub fn config(message: impl Into<String>, fix_hint: impl Into<String>) -> Self {
        Self::Config {
            message: message.into(),
            fix_hint: fix_hint.into(),
        }
    }
}

pub type Result<T> = std::result::Result<T, SheetkeelError>;
